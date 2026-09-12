# WEB-004 review cycle

**Status:** Fixed and verified
**Branch:** `web/web-004-sse`

## Resolution

| | |
|---|---|
| **Outcome** | Delivered as specified |
| **Agent time** | none, implemented directly in session |
| **Patches rejected on review** | 0 |
| **Suite** | 617 to 629 |
| **Follow-ups** | none |

## The finding held up

The ticket predicted that a channel-backed `Read` handed to `tiny_http::Response` would deliver
nothing until the 8 KB chunked-transfer buffer filled. That was checked rather than assumed: the
naive version was written out in full, a `ChanRead` streaming frames into
`Response::new(.., data_length: None, ..)`, and `first_event_arrives_promptly` timed out with **no
event reaching the socket at all** inside 2.5 seconds.

So `sse::serve` takes `Request::into_writer()` and writes its own chunk framing with a flush per
event. The reason is a comment at the top of the module, because the shape of that code invites
exactly the simplification that breaks it.

## What changed

**`Hub`.** Clients behind bounded `SyncSender`s, a 64-frame replay ring, and `MAX_CLIENTS = 8`. The
broadcaster only ever `try_send`s, so a phone that stopped reading cannot hold the publisher up; a
client whose queue fills is dropped, and EventSource brings it straight back with a `Last-Event-ID`.
A reconnect inside the ring is told only what it missed, outside it gets `event: reset` and a full
snapshot.

**The broadcaster polls the published state** at 200ms rather than being called by the publisher.
That keeps `web::publish` unaware of who is connected, and at 200ms against a 500ms publish it adds
less latency than the interval it is watching.

**The page** now subscribes instead of polling, merges panes field by field (a pane arrives whole or
not at all, so there is no patch to apply), throws everything away when `gen` changes, and reconnects
deliberately on `visibilitychange`, because iOS kills a backgrounded stream and sometimes hands back
a dead one.

## What was rejected

**Waking the broadcaster from the publisher.** It would cut ~200ms of latency on a feature whose
underlying data moves on a 1500ms log poll, in exchange for the publisher holding a reference to the
connection list. Not worth the coupling.

**Long-polling as the default.** Each cycle is a fresh connection and a fresh thread, which on a
phone is a wakeup per cycle.

## How the tests were proven to have teeth

| Reverted | Result |
|---|---|
| `sse::serve` rewritten as `Response` + a channel-backed `Read` | `first_event_arrives_promptly` fails: no event reached the socket in 2.5s |

The rest of the module is covered by assertions that are their own teeth: byte-exact chunk framing
(`2\r\nhi\r\n`), the frame shapes EventSource actually parses, ring eviction behaviour at the
boundary, the client cap, and a stalled client being dropped rather than queued.

`a_client_that_stops_reading_is_dropped_rather_than_queued` is worth keeping in mind when reading the
cap: the two together are the answer to `tiny_http` exposing no socket handle, so no send timeout can
be set on a stream. Nothing here can stop a wedged socket from holding its thread; it can only bound
how many of them there are.

## `/api/state` was in the deliverable and was not built

The ticket asked for `GET /api/state?since=N` as a fallback alongside the stream. It was not
implemented, and the review above discussed it as though it were an option under consideration rather
than something promised and skipped. That is the failure worth naming: the wording made a gap read
like a decision.

It exists now, with one deliberate narrowing. The ticket said long-polling; it answers immediately
instead, returning whatever changed since `since` and closing. There are four workers, so a handful
of parked phones would starve every other request on the server, and `tiny_http` gives no way to set
a send timeout on a held socket. A client polling this every couple of seconds gets the same result
without that risk. `state_answers_a_delta_and_does_not_hold_the_connection` asserts both halves,
including that it returns in well under a second.
