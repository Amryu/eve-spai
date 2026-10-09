//! Server-sent events off a blocking reader, one event at a time.
//!
//! The model APIs stream their answers this way. The reader is checked for cancellation between
//! reads, so stopping a turn does not wait for the server to finish.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    pub event: String,
    pub data: String,
}

pub struct SseReader<R: Read> {
    inner: R,
    buf: Vec<u8>,
    done: bool,
}

impl<R: Read> SseReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, buf: Vec::new(), done: false }
    }

    /// The next event, or `None` at the end of the stream or once `cancel` is set.
    pub fn next(&mut self, cancel: &AtomicBool) -> Option<Event> {
        loop {
            if let Some(ev) = self.take_event() {
                return Some(ev);
            }
            if self.done || cancel.load(Ordering::Relaxed) {
                return None;
            }
            let mut chunk = [0u8; 8192];
            match self.inner.read(&mut chunk) {
                Ok(0) | Err(_) => {
                    self.done = true;
                    // A last event without its blank line still counts.
                    if !self.buf.is_empty() {
                        self.buf.extend_from_slice(b"\n\n");
                    }
                }
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
            }
        }
    }

    fn take_event(&mut self) -> Option<Event> {
        let text = String::from_utf8_lossy(&self.buf);
        let norm = text.replace("\r\n", "\n");
        let end = norm.find("\n\n")?;
        let block = norm[..end].to_owned();
        let consumed = byte_len_through(&self.buf, end + 2);
        self.buf.drain(..consumed);
        let mut ev = Event::default();
        let mut data: Vec<&str> = Vec::new();
        for line in block.lines() {
            if let Some(v) = line.strip_prefix("event:") {
                ev.event = v.trim().to_owned();
            } else if let Some(v) = line.strip_prefix("data:") {
                data.push(v.strip_prefix(' ').unwrap_or(v));
            }
        }
        ev.data = data.join("\n");
        if ev.event.is_empty() && ev.data.is_empty() {
            return self.take_event();
        }
        Some(ev)
    }
}

/// How many raw bytes make up the first `chars_through` bytes of the CRLF-normalised text: a
/// `\r\n` counts as one there but two here.
fn byte_len_through(raw: &[u8], normalised: usize) -> usize {
    let mut n = 0;
    let mut i = 0;
    while i < raw.len() && n < normalised {
        if raw[i] == b'\r' && raw.get(i + 1) == Some(&b'\n') {
            i += 2;
        } else {
            i += 1;
        }
        n += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Trickle(Vec<u8>, usize);
    impl Read for Trickle {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            // Three bytes at a time, so events and UTF-8 characters arrive split.
            let n = 3.min(self.0.len() - self.1).min(out.len());
            out[..n].copy_from_slice(&self.0[self.1..self.1 + n]);
            self.1 += n;
            Ok(n)
        }
    }

    #[test]
    fn events_survive_split_reads_and_crlf() {
        let raw = "event: a\r\ndata: {\"x\":\"héllo\"}\r\n\r\n: comment\n\nevent: b\ndata: 1\ndata: 2\n\ndata: tail";
        let mut r = SseReader::new(Trickle(raw.as_bytes().to_vec(), 0));
        let stop = AtomicBool::new(false);
        assert_eq!(r.next(&stop), Some(Event { event: "a".into(), data: "{\"x\":\"héllo\"}".into() }));
        assert_eq!(r.next(&stop), Some(Event { event: "b".into(), data: "1\n2".into() }));
        assert_eq!(r.next(&stop), Some(Event { event: String::new(), data: "tail".into() }));
        assert_eq!(r.next(&stop), None);
    }

    #[test]
    fn a_cancelled_stream_stops_reading() {
        let mut r = SseReader::new(Trickle(b"data: 1\n\ndata: 2\n\n".to_vec(), 0));
        let stop = AtomicBool::new(true);
        assert_eq!(r.next(&stop), None);
    }
}
