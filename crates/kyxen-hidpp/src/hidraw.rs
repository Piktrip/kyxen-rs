//! `/dev/hidraw*` transport.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::fs::OFlags;

use crate::device::Transport;

/// An open hidraw node. HID++ software-mode settings (G-key diversion) are
/// tied to the open file and revert when it is closed.
#[derive(Debug)]
pub struct Hidraw {
    file: File,
    path: PathBuf,
}

impl Hidraw {
    pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(OFlags::NONBLOCK.bits().cast_signed())
            .open(&path)?;
        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Transport for Hidraw {
    fn send(&mut self, report: &[u8]) -> io::Result<()> {
        loop {
            match self.file.write(report) {
                Ok(_) => return Ok(()),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    wait(&self.file, PollFlags::OUT, None)?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize> {
        loop {
            match self.file.read(buf) {
                Ok(n) => return Ok(n),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if !wait(&self.file, PollFlags::IN, Some(timeout))? {
                        return Ok(0);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// Wait until `file` is ready for `flags`; `false` on timeout.
fn wait(file: &File, flags: PollFlags, timeout: Option<Duration>) -> io::Result<bool> {
    let ts = timeout.map(|t| {
        Timespec::try_from(t).unwrap_or(Timespec {
            tv_sec: i64::MAX,
            tv_nsec: 0,
        })
    });
    let mut fds = [PollFd::new(file, flags)];
    Ok(poll(&mut fds, ts.as_ref())? > 0)
}
