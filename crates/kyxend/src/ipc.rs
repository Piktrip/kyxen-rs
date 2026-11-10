//! Unix-socket control interface, wire-compatible with Python Kyxen's GUI:
//! one JSON request per connection, one JSON response.
//!
//! Protocol: see [`kyxen_core::ipc`].

use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use crossbeam_channel::Sender;
use serde_json::{Value, json};
use tracing::{debug, warn};

use crate::daemon::Event;

pub use kyxen_core::ipc::{Request, socket_path};

/// Bind the socket; fails if another daemon is already answering on it.
pub fn bind(path: &Path) -> Result<UnixListener> {
    if UnixStream::connect(path).is_ok() {
        bail!(
            "another kyxen daemon is already running ({})",
            path.display()
        );
    }
    match fs::remove_file(path) {
        Err(e) if e.kind() != ErrorKind::NotFound => {
            return Err(e).with_context(|| format!("remove stale {}", path.display()));
        }
        _ => {}
    }
    let listener = UnixListener::bind(path).with_context(|| format!("bind {}", path.display()))?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

pub fn spawn(listener: UnixListener, events: Sender<Event>) -> Result<JoinHandle<()>> {
    Ok(thread::Builder::new().name("ipc".into()).spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let events = events.clone();
                    thread::spawn(move || {
                        if let Err(e) = serve(stream, &events) {
                            debug!("ipc client: {e:#}");
                        }
                    });
                }
                Err(e) => warn!("ipc accept failed: {e}"),
            }
        }
    })?)
}

/// Read until the bytes form a complete JSON value (clients don't send EOF).
fn read_request(stream: &mut UnixStream) -> Result<Value> {
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            bail!("connection closed before a complete request");
        }
        buf.extend_from_slice(&chunk[..n]);
        if let Ok(v) = serde_json::from_slice(&buf) {
            return Ok(v);
        }
        if buf.len() > 64 * 1024 {
            bail!("request too large");
        }
    }
}

fn serve(mut stream: UnixStream, events: &Sender<Event>) -> Result<()> {
    let value = read_request(&mut stream)?;
    let response = match serde_json::from_value::<Request>(value) {
        Ok(req) => {
            let (tx, rx) = crossbeam_channel::bounded(1);
            events.send(Event::Ipc(req, tx))?;
            rx.recv_timeout(Duration::from_secs(2))
                .unwrap_or_else(|_| json!({"error": "daemon busy"}))
        }
        Err(_) => json!({"error": "unknown command"}),
    };
    stream.write_all(response.to_string().as_bytes())?;
    Ok(())
}

/// Client side, used by `kyxend status|switch|reload`.
pub fn request(path: &Path, req: &Request) -> Result<Value> {
    kyxen_core::ipc::request(path, req)
        .with_context(|| format!("talk to {} — is kyxend running?", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_over_socket() {
        let path = std::env::temp_dir().join(format!("kyxen-ipc-{}.sock", std::process::id()));
        let listener = bind(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let (tx, rx) = crossbeam_channel::unbounded();
        spawn(listener, tx).unwrap();
        thread::spawn(move || {
            if let Ok(Event::Ipc(Request::Status, reply)) = rx.recv() {
                reply
                    .send(json!({"active_profile": "test", "profiles": ["test"], "keyboard": null}))
                    .unwrap();
            }
        });
        let resp = request(&path, &Request::Status).unwrap();
        assert_eq!(resp["active_profile"], "test");
        assert!(
            bind(&path).is_err(),
            "second daemon must not take over the socket"
        );
        fs::remove_file(&path).unwrap();
    }
}
