//! Single instance: a second `kyxen` brings the running window to the front.

use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::thread;

use crate::AppWindow;

fn addr() -> std::io::Result<SocketAddr> {
    let user = std::env::var("USER").unwrap_or_default();
    SocketAddr::from_abstract_name(format!("kyxen-gui-{user}"))
}

/// Ask a running instance to show its window. `true` if one answered.
pub fn signal_running() -> bool {
    let Ok(addr) = addr() else { return false };
    UnixStream::connect_addr(&addr)
        .and_then(|mut s| s.write_all(b"show"))
        .is_ok()
}

/// Listen for other instances; returns `false` if the name is taken.
pub fn listen(ui: slint::Weak<AppWindow>) -> bool {
    let Ok(listener) = addr().and_then(|a| UnixListener::bind_addr(&a)) else {
        return false;
    };
    thread::spawn(move || {
        for mut conn in listener.incoming().flatten() {
            let mut buf = [0u8; 8];
            if conn.read(&mut buf).is_ok_and(|n| &buf[..n] == b"show") {
                let _ = ui.upgrade_in_event_loop(|w| crate::present(&w));
            }
        }
    });
    true
}
