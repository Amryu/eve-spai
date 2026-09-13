# WEB-043 &mdash; A route says nothing about what is waiting on it

| | |
|---|---|
| **Severity** | Medium |
| **Status** | Open |
| **Region** | `web/route.rs`, `assets/dialogs.*`, `app.rs` |
| **Reported by** | user |

## Symptoms

1. "Route planner: add warnings about systems that had danger level and above intel messages, or
   recent kills."
2. "Using the controls on the jump route window can accidentally grab and move the window"
3. "The jump route and system window need a much more compact version for mobile."

## Gap

A route was a list of names and distances. Everything needed to say which of those systems someone
had just been killed in was already on both sides; nothing joined the two up.

2 is the drag handler: it let go of the pointer for a `button`, an `a` and an `input`, and a
drop-down is none of those, so changing the hull dragged the window instead.

## How to verify

A route through a system with a Danger report inside the TTL, and one with kills this hour. Warning on
everything below Danger would be WRONG: a nullsec route passes through dozens of systems someone has
said something about.
