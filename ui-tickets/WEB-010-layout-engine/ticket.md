# WEB-010 &mdash; The panes have no arrangement the user controls

| | |
|---|---|
| **Severity** | Feature |
| **Status** | Open |
| **Region** | `assets/layout.{js,css}`, the container rules in `app.css` |
| **Reported by** | user, remote web view |

## Gap

A phone wants one pane at a time with a swipe between them. A second monitor wants four columns. The
page has one fixed arrangement.

## Deliverable

Layout state `{ mode: auto | tabs | columns | grid, order, spans, active }` persisted in
`localStorage`, per device. `Settings.web.default_layout` seeds only the first visit.

- **tabs**: a native scroll-snap strip, `scroll-snap-type: x mandatory` with each pane
  `min-width: 100%`, so swipe, momentum and rubber-banding are the browser's job. The tab bar calls
  `scrollIntoView({behavior:'smooth'})`, and a debounced scroll listener syncs the active tab back.
- **columns**: `grid-template-columns: repeat(N, 1fr)` with a per-pane span.
- **grid**: `repeat(2, 1fr)` with `grid-auto-rows: 1fr`.
- **auto**: `matchMedia('(min-width:900px)')` picks columns, below it picks tabs.

Drag to reorder: HTML5 drag on the tab chips for pointer devices, long-press plus Pointer Events for
touch. Mutates `order`, re-renders, persists.

## Notes

Per device, not in `Settings`. A phone and a desktop browser want different layouts and the app must
not fight them.

Scroll-snap rather than hand-rolled `touchstart`/`touchmove`/`touchend`. Hand-rolled swipe gets
momentum, over-scroll and interrupted gestures wrong, and it fights the browser on iOS.

Each pane stays a pure `render(el, snapshot)`; the layout engine only decides what is mounted and how
the container is styled. A pane that reaches into the layout is a pane that cannot be moved.

## How to verify

- WEB-005 demo screenshots in all three modes at 390px, 900px and 1440px.
- Reorder a pane, reload, confirm the order survived; switch device, confirm it did not follow.
- A fix would be WRONG if it stored layout in `Settings` and pushed it to every device, or if a pane's
  own module started reading the layout state.
