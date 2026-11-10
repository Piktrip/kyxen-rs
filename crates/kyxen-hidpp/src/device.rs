use std::collections::{HashMap, VecDeque};
use std::io;
use std::time::{Duration, Instant};

use crate::error::{Error, Result};
use crate::report::{self, Message, Notification, ReportKind, SW_ID};

/// Default time to wait for a response.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(3);

/// Raw report I/O with a keyboard's HID++ interface.
pub trait Transport {
    /// Send one report.
    fn send(&mut self, report: &[u8]) -> io::Result<()>;

    /// Wait up to `timeout` for one report and copy it into `buf`.
    /// Returns the report length, or `0` if nothing arrived in time.
    fn recv(&mut self, buf: &mut [u8], timeout: Duration) -> io::Result<usize>;
}

/// A keyboard reached over HID++ 2.0.
///
/// Requests wait for the matching response; notifications that arrive in the
/// meantime are buffered and handed out by [`HidppDevice::next_notification`].
#[derive(Debug)]
pub struct HidppDevice<T> {
    transport: T,
    features: HashMap<u16, u8>,
    pending: VecDeque<Notification>,
    timeout: Duration,
}

impl<T: Transport> HidppDevice<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            features: HashMap::new(),
            pending: VecDeque::new(),
            timeout: DEFAULT_TIMEOUT,
        }
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    pub(crate) fn feature_cache(&mut self) -> &mut HashMap<u16, u8> {
        &mut self.features
    }

    /// Send a long request and return the 16-byte response payload.
    pub fn request(&mut self, feature: u8, function: u8, params: &[u8]) -> Result<Vec<u8>> {
        self.request_with(ReportKind::Long, feature, function, params)
    }

    /// Send a request as `kind` and wait for the matching response.
    pub fn request_with(
        &mut self,
        kind: ReportKind,
        feature: u8,
        function: u8,
        params: &[u8],
    ) -> Result<Vec<u8>> {
        self.transport
            .send(&report::request(kind, feature, function, params))?;
        let deadline = Instant::now() + self.timeout;
        let mut buf = [0u8; 64];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::Timeout { feature, function });
            }
            let n = self.transport.recv(&mut buf, left)?;
            if n == 0 {
                return Err(Error::Timeout { feature, function });
            }
            match Message::parse(&buf[..n]) {
                Some(Message::Response {
                    feature: f,
                    function: func,
                    sw_id,
                    payload,
                }) if f == feature && func == function && sw_id == SW_ID => {
                    return Ok(payload);
                }
                Some(Message::Error {
                    feature: f,
                    function: func,
                    code,
                }) if f == feature && func == function => {
                    return Err(Error::Hidpp {
                        feature,
                        function,
                        code: code.into(),
                    });
                }
                Some(Message::Notification(n)) => self.pending.push_back(n),
                // Responses to other requests, or non-HID++ reports.
                _ => {}
            }
        }
    }

    /// Send a report without waiting for an answer (lighting frames).
    pub fn send_raw(&mut self, report: &[u8]) -> Result<()> {
        Ok(self.transport.send(report)?)
    }

    /// Return the next notification, waiting up to `timeout` for one.
    /// Responses and non-HID++ reports that arrive meanwhile are dropped.
    pub fn next_notification(&mut self, timeout: Duration) -> Result<Option<Notification>> {
        if let Some(n) = self.pending.pop_front() {
            return Ok(Some(n));
        }
        let deadline = Instant::now() + timeout;
        let mut buf = [0u8; 64];
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let n = self.transport.recv(&mut buf, left)?;
            if n == 0 {
                return Ok(None);
            }
            if let Some(Message::Notification(note)) = Message::parse(&buf[..n]) {
                return Ok(Some(note));
            }
            if left.is_zero() {
                return Ok(None);
            }
        }
    }
}
