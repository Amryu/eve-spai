//! Server-sent events.
//!
//! # Why this writes its own chunks
//!
//! `tiny_http::Response::raw_print` sends an unknown-length body through
//! `chunked_transfer::Encoder`, which buffers 8192 bytes with `flush_after_write: false`, and
//! `io::copy` never flushes. A `Response` with `data_length: None` puts nothing on the wire until
//! about forty events queue up.
//!
//! `Request::into_writer` hands back the raw socket, and `Drop for Request` is a no-op once the
//! writer is taken. The framing below is what the encoder would write, plus flushes.
//! `first_event_arrives_promptly` fails if this goes back to `Response`.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::state::SharedWeb;

/// Live streams allowed at once. `tiny_http` exposes no socket handle for a send timeout, so a phone
/// that leaves wifi blocks a thread in write while the kernel retransmits. The cap bounds that.
pub const MAX_CLIENTS: usize = 8;

/// Frames a client may fall behind before it is dropped. Small, because a reconnect is cheaper than
/// a growing backlog.
const QUEUE: usize = 8;

/// Frames kept for replay. A reconnect inside this window gets only what it missed, outside it a
/// full snapshot.
const RING: usize = 64;

const POLL: Duration = Duration::from_millis(200);
const KEEPALIVE: Duration = Duration::from_secs(15);

pub enum Frame {
    Event { id: u64, json: Arc<str> },
    Bye(&'static str),
}

struct Client {
    id: u64,
    tx: SyncSender<Frame>,
}

#[derive(Default)]
pub struct Hub {
    clients: Mutex<Vec<Client>>,
    ring: Mutex<std::collections::VecDeque<(u64, Arc<str>)>>,
    next_id: AtomicU64,
}

pub type SharedHub = Arc<Hub>;

impl Hub {
    pub fn client_count(&self) -> usize {
        self.clients.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// `None` when the cap is reached, which the caller answers with 503.
    fn join(&self) -> Option<(u64, Receiver<Frame>)> {
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        if clients.len() >= MAX_CLIENTS {
            return None;
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let (tx, rx) = sync_channel(QUEUE);
        clients.push(Client { id, tx });
        Some((id, rx))
    }

    fn leave(&self, id: u64) {
        self.clients.lock().unwrap_or_else(|e| e.into_inner()).retain(|c| c.id != id);
    }

    fn replay(&self, since: u64) -> Option<Vec<(u64, Arc<str>)>> {
        let ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
        let oldest = ring.front().map(|(id, _)| *id)?;
        // `since + 1` is the first frame the client still needs.
        if oldest > since + 1 {
            return None;
        }
        Some(ring.iter().filter(|(id, _)| *id > since).cloned().collect())
    }

    /// Tell every stream to reconnect. A restart drops the listener but not the stream threads, and
    /// `EventSource` only reconnects when the stream ends.
    pub fn close_all(&self, why: &'static str) {
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        for c in clients.drain(..) {
            let _ = c.tx.try_send(Frame::Bye(why));
        }
    }

    fn broadcast(&self, id: u64, json: Arc<str>) {
        {
            let mut ring = self.ring.lock().unwrap_or_else(|e| e.into_inner());
            ring.push_back((id, json.clone()));
            while ring.len() > RING {
                ring.pop_front();
            }
        }
        let mut clients = self.clients.lock().unwrap_or_else(|e| e.into_inner());
        // `try_send` so the publisher never blocks behind a phone that stopped reading.
        clients.retain(|c| {
            !matches!(
                c.tx.try_send(Frame::Event { id, json: json.clone() }),
                Err(TrySendError::Disconnected(_)) | Err(TrySendError::Full(_))
            )
        });
    }
}

/// Polls the published state and fans out what changed, so the publisher knows nothing about
/// connected clients.
pub fn spawn_broadcaster(hub: SharedHub, web: SharedWeb) {
    let _ = std::thread::Builder::new().name("web-sse".into()).spawn(move || {
        let mut sent = 0u64;
        loop {
            std::thread::sleep(POLL);
            let payload = {
                let st = web.lock().unwrap_or_else(|e| e.into_inner());
                (st.seq > sent).then(|| {
                    let snap = st.snapshot_since(sent);
                    (st.seq, serde_json::to_string(&snap).unwrap_or_default())
                })
            };
            if let Some((seq, json)) = payload {
                sent = seq;
                hub.broadcast(seq, json.into());
            }
        }
    });
}

fn chunk(w: &mut dyn Write, body: &str) -> std::io::Result<()> {
    write!(w, "{:x}\r\n", body.len())?;
    w.write_all(body.as_bytes())?;
    w.write_all(b"\r\n")?;
    w.flush()
}

pub fn head() -> &'static str {
    "HTTP/1.1 200 OK\r\n\
     Content-Type: text/event-stream\r\n\
     Cache-Control: no-store\r\n\
     Transfer-Encoding: chunked\r\n\
     Connection: close\r\n\
     X-Accel-Buffering: no\r\n\
     \r\n"
}

pub fn event(id: u64, json: &str) -> String {
    format!("id: {id}\ndata: {json}\n\n")
}

pub fn full_reset(id: u64, json: &str) -> String {
    format!("event: reset\nid: {id}\ndata: {json}\n\n")
}

pub const KEEPALIVE_FRAME: &str = ": keepalive\n\n";

/// Streams on its own thread so it does not occupy one of the server's few workers.
pub fn serve(req: tiny_http::Request, hub: SharedHub, web: SharedWeb, last_event_id: Option<u64>) {
    let Some((id, rx)) = hub.join() else {
        let _ = req.respond(
            tiny_http::Response::from_string("too many streams\n").with_status_code(503).with_header(
                tiny_http::Header::from_bytes(&b"Retry-After"[..], &b"5"[..]).expect("static"),
            ),
        );
        return;
    };

    std::thread::spawn(move || {
        let mut w = req.into_writer();
        if w.write_all(head().as_bytes()).and_then(|()| w.flush()).is_err() {
            hub.leave(id);
            return;
        }
        // iOS kills the stream when the tab backgrounds, so reconnecting is the normal path.
        if chunk(&mut *w, "retry: 2000\n\n").is_err() {
            hub.leave(id);
            return;
        }

        let backlog = last_event_id.and_then(|since| hub.replay(since));
        let opening = match backlog {
            Some(frames) => frames.iter().map(|(i, j)| event(*i, j)).collect::<String>(),
            None => {
                let (seq, json) = {
                    let mut st = web.lock().unwrap_or_else(|e| e.into_inner());
                    (st.seq, st.full_json())
                };
                full_reset(seq, &json)
            }
        };
        if !opening.is_empty() && chunk(&mut *w, &opening).is_err() {
            hub.leave(id);
            return;
        }

        loop {
            let body = match rx.recv_timeout(KEEPALIVE) {
                Ok(Frame::Event { id, json }) => event(id, &json),
                Ok(Frame::Bye(why)) => {
                    let _ = chunk(&mut *w, &format!("event: bye\ndata: {why}\n\n"));
                    break;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => KEEPALIVE_FRAME.to_owned(),
                Err(_) => break,
            };
            if chunk(&mut *w, &body).is_err() {
                break;
            }
        }
        let _ = w.write_all(b"0\r\n\r\n");
        hub.leave(id);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_framing_is_byte_exact() {
        let mut buf: Vec<u8> = Vec::new();
        chunk(&mut buf, "hi").unwrap();
        assert_eq!(buf, b"2\r\nhi\r\n", "length in hex, then the body, then CRLF");

        buf.clear();
        chunk(&mut buf, &"x".repeat(255)).unwrap();
        assert!(buf.starts_with(b"ff\r\n"), "hex, not decimal");
    }

    #[test]
    fn frames_are_shaped_the_way_eventsource_reads_them() {
        assert_eq!(event(7, "{\"a\":1}"), "id: 7\ndata: {\"a\":1}\n\n");
        assert_eq!(full_reset(3, "{}"), "event: reset\nid: 3\ndata: {}\n\n");
        assert!(KEEPALIVE_FRAME.starts_with(':'), "a comment, which EventSource ignores");
        assert!(KEEPALIVE_FRAME.ends_with("\n\n"), "and still a complete frame");
        assert!(head().contains("text/event-stream"));
        assert!(head().contains("X-Accel-Buffering: no"), "a proxy must not buffer this");
    }

    fn ring_hub(seqs: &[u64]) -> Hub {
        let hub = Hub::default();
        for s in seqs {
            hub.broadcast(*s, format!("{{\"seq\":{s}}}").into());
        }
        hub
    }

    #[test]
    fn a_reconnect_inside_the_ring_is_told_only_what_it_missed() {
        let hub = ring_hub(&[1, 2, 3, 4]);
        let frames = hub.replay(2).expect("2 is still in the ring");
        assert_eq!(frames.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![3, 4]);
    }

    #[test]
    fn a_reconnect_that_fell_out_of_the_ring_asks_for_everything() {
        let seqs: Vec<u64> = (1..=(RING as u64 + 10)).collect();
        let hub = ring_hub(&seqs);
        assert!(hub.replay(1).is_none(), "frame 2 has been evicted, so the gap cannot be filled");
        let newest = *seqs.last().unwrap();
        assert!(hub.replay(newest - 1).is_some(), "a recent client is still served from the ring");
    }

    #[test]
    fn a_caught_up_reconnect_replays_nothing() {
        let hub = ring_hub(&[1, 2, 3]);
        assert_eq!(hub.replay(3).expect("in range").len(), 0);
    }

    #[test]
    fn an_empty_ring_sends_a_full_snapshot() {
        assert!(Hub::default().replay(0).is_none());
    }

    #[test]
    fn the_cap_is_enforced_and_leaving_frees_a_slot() {
        let hub = Hub::default();
        let mut held = Vec::new();
        for _ in 0..MAX_CLIENTS {
            held.push(hub.join().expect("under the cap"));
        }
        assert_eq!(hub.client_count(), MAX_CLIENTS);
        assert!(hub.join().is_none(), "the cap bounds how many sockets can wedge a thread");

        let (id, _rx) = held.pop().unwrap();
        hub.leave(id);
        assert!(hub.join().is_some(), "a departed client frees its slot");
    }

    #[test]
    fn closing_tells_every_stream_to_come_back() {
        let hub = Hub::default();
        let held: Vec<_> = (0..3).map(|_| hub.join().expect("joined")).collect();
        hub.close_all("restart");
        assert_eq!(hub.client_count(), 0, "the hub lets go of them");
        for (_, rx) in &held {
            assert!(
                matches!(rx.try_recv(), Ok(Frame::Bye("restart"))),
                "each stream has to be told, or the phone sits on a dead socket"
            );
        }
    }

    /// A dropped client comes back through EventSource with a `Last-Event-ID`.
    #[test]
    fn a_client_that_stops_reading_is_dropped_rather_than_queued() {
        let hub = Hub::default();
        let (_id, rx) = hub.join().expect("joined");
        for s in 1..=(QUEUE as u64 + 2) {
            hub.broadcast(s, "{}".into());
        }
        assert_eq!(hub.client_count(), 0, "the stalled client was dropped");
        drop(rx);
    }
}
