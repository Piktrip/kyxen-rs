//! Daemon control protocol (client side and shared types).
//!
//! One JSON request per connection on a Unix socket, one JSON response.
//! Wire-compatible with Python Kyxen for `status`, `switch` and `reload`.
//!
//! ```text
//! {"cmd": "status"}                 → {"active_profile", "profiles", "keyboard"}
//! {"cmd": "switch", "profile": "x"} → {"ok": bool}
//! {"cmd": "reload"}                 → {"ok": true}
//! {"cmd": "run", "gkey": 2}         → {"ok": bool}   (fire G2 of the active profile)
//! ```

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Same path as Python Kyxen so its GUI keeps working.
pub const SOCKET: &str = "/tmp/kyxen.sock";

pub fn socket_path() -> PathBuf {
    std::env::var_os("KYXEN_SOCKET").map_or_else(|| PathBuf::from(SOCKET), PathBuf::from)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Status,
    Switch {
        #[serde(default)]
        profile: String,
    },
    Reload,
    /// Run G-key `gkey` (1-based) of the active profile once.
    Run {
        gkey: u8,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Status {
    pub active_profile: String,
    pub profiles: Vec<String>,
    /// Model name, or `None` while no keyboard is connected.
    pub keyboard: Option<String>,
}

/// Send `req` to the daemon at `path` and return its JSON response.
pub fn request(path: &Path, req: &Request) -> io::Result<Value> {
    let mut s = UnixStream::connect(path)?;
    s.set_read_timeout(Some(Duration::from_secs(3)))?;
    s.set_write_timeout(Some(Duration::from_secs(3)))?;
    s.write_all(serde_json::to_string(req)?.as_bytes())?;
    let mut out = Vec::new();
    s.read_to_end(&mut out)?;
    Ok(serde_json::from_slice(&out)?)
}

/// Daemon status, or `None` if no daemon answers.
pub fn status() -> Option<Status> {
    serde_json::from_value(request(&socket_path(), &Request::Status).ok()?).ok()
}

/// Send a command that answers `{"ok": bool}`; `false` if it failed or no daemon runs.
pub fn command(req: &Request) -> bool {
    request(&socket_path(), req).is_ok_and(|v| v["ok"] == true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_format() {
        assert_eq!(
            serde_json::to_string(&Request::Status).unwrap(),
            r#"{"cmd":"status"}"#
        );
        assert_eq!(
            serde_json::to_string(&Request::Run { gkey: 2 }).unwrap(),
            r#"{"cmd":"run","gkey":2}"#
        );
        let r: Request = serde_json::from_str(r#"{"cmd": "switch", "profile": "gaming"}"#).unwrap();
        assert_eq!(
            r,
            Request::Switch {
                profile: "gaming".into()
            }
        );
        assert!(serde_json::from_str::<Request>(r#"{"cmd":"explode"}"#).is_err());
        let s: Status =
            serde_json::from_str(r#"{"active_profile":"a","profiles":["a"],"keyboard":null}"#)
                .unwrap();
        assert_eq!(s.keyboard, None);
    }
}
