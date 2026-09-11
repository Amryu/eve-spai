# WEB-004 &mdash; The page has no way to learn that anything changed

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `web/sse.rs` |
| **Reported by** | user, remote web view |

## Gap

WEB-002 produces a snapshot and WEB-003 can serve it once. Intel is worthless a minute late, so the
page needs a push channel.

## Deliverable

`GET /api/events` as SSE, plus `GET /api/state?since=N` long-polling the same serializer as a fallback.
A 64-frame replay ring keyed by `seq`, `Last-Event-ID` honoured, `retry: 2000`, a 15s keepalive
comment, a hard cap of 8 live clients with `503` and `Retry-After` beyond it. Each client holds a
bounded `SyncSender`; the publisher only ever `try_send`s and marks a full client stale.

## Measured

`tiny_http::Response::raw_print` (`response.rs:432`) sends an unknown-length body with
`io::copy(&mut reader, &mut Encoder::new(writer))`. `chunked_transfer::Encoder::new` takes an
8192-byte buffer with `flush_after_write: false` (`chunked_transfer-1.5.0/src/encoder.rs:52,138`) and
its `write` only calls `send()` when that buffer overflows. `io::copy` never calls `flush`.

| | |
|---|---|
| Bytes on the wire before the buffer fills | 0 |
| SSE frame size, one intel snapshot | ~200 bytes |
| Events queued before the first byte reaches the browser | ~40 |

## Cause

A channel-backed `Read` handed to `Response::new(.., data_length: None, ..)` therefore delivers
nothing until roughly forty events have accumulated, and it presents as a browser or network fault
rather than as a server bug.

## Notes

Use `Request::into_writer()` (`tiny_http/src/request.rs:390`). It returns the raw
`Box<dyn Write + Send>`, and `extract_writer_impl` clears `response_writer`, so `Drop for Request`
(`request.rs:487`) becomes a no-op and does not emit a 500 behind you. Write the chunk framing by
hand and flush per event:

```rust
write!(w, "{:x}\r\n", body.len())?; w.write_all(body.as_bytes())?; w.write_all(b"\r\n")?; w.flush()
```

Put a comment at that call site naming the buffering reason, or the next person will "simplify" it
back to `Response`.

Hand each `/api/events` request to its own detached thread so it never occupies a worker. `tiny_http`
exposes no socket handle, so no send timeout can be set on it; the client cap, the bounded channel and
the keepalive are what bound the damage from a phone that walked out of wifi.

iOS Safari kills the EventSource on background and reconnects on foreground. `retry` plus
`Last-Event-ID` covers it; the page should also close and reopen on `visibilitychange`, because
Safari sometimes hands back a zombie.

## How to verify

- **The latency test.** Connect a raw `TcpStream` to a test server, publish, and assert the first
  `data:` line arrives within 500ms. This test fails on the naive `Response` implementation, which is
  the entire point of writing it.
- Byte-exact assertions on `chunk()` output and on the keepalive comment frame.
- Ninth client gets 503. `Last-Event-ID` inside the ring replays from there; outside it forces a full
  resync.
- A fix would be WRONG if it made the latency test pass by shrinking the encoder's chunk size rather
  than by controlling the flush.
