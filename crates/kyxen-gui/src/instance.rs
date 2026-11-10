//! Single instance: a second `kyxen` brings the running window to the front.
//!
//! Protocol: the second instance connects and sends `show`, optionally
//! followed by a space and its XDG activation token, then closes.

use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::thread;

use crate::AppWindow;

fn addr() -> std::io::Result<SocketAddr> {
    let user = std::env::var("USER").unwrap_or_default();
    SocketAddr::from_abstract_name(format!("kyxen-gui-{user}"))
}

/// Ask a running instance to show its window, passing on our activation
/// token. `true` if one answered.
pub fn signal_running(token: Option<&str>) -> bool {
    let Ok(addr) = addr() else { return false };
    let msg = match token {
        Some(t) => format!("show {t}"),
        None => "show".to_owned(),
    };
    UnixStream::connect_addr(&addr)
        .and_then(|mut s| s.write_all(msg.as_bytes()))
        .is_ok()
}

/// Listen for other instances; returns `false` if the name is taken.
pub fn listen(ui: slint::Weak<AppWindow>) -> bool {
    let Ok(listener) = addr().and_then(|a| UnixListener::bind_addr(&a)) else {
        return false;
    };
    thread::spawn(move || {
        for conn in listener.incoming().flatten() {
            let mut msg = String::new();
            if conn.take(1024).read_to_string(&mut msg).is_err() {
                continue;
            }
            let mut parts = msg.splitn(2, ' ');
            if parts.next() != Some("show") {
                continue;
            }
            let token = parts.next().map(str::trim);
            tracing::debug!("show requested, activation token: {token:?}");
            if let Some(token) = token {
                crate::activation::set(token.to_owned());
            }
            let _ = ui.upgrade_in_event_loop(|w| crate::present(&w));
        }
    });
    true
}
