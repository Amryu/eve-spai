# EVE Spai

EVE Spai is an intel and situational-awareness tool for EVE Online. It watches your
local EVE chat logs, parses intel as it is posted, places hostiles on a star map,
raises configurable alerts, and brings fleet communications and killboard context into
one window. It is a desktop application written in Rust using the egui toolkit.

It is a  project inspired by . It reuses only static EVE data and
locations (system names, map layout, ship and wormhole reference data from CCP's
public data exports), not any third-party application code.

## Status and platform support

The application is developed and tested on Linux. The core (intel parsing, map, ESI,
killboard, XMPP, persistence, notifications, self-update) is portable, and the project
builds with the standard Rust toolchain. A few integrations are currently Linux first:

- Alert sound playback uses the system audio players (PulseAudio or ALSA). On other
  platforms the tones are still synthesised but may not play until a native player is
  wired in.
- The system tray and login autostart use the Linux StatusNotifierItem and XDG
  autostart conventions. On other platforms these features are cleanly disabled.
- The "smart" always-on-top map overlay uses X11 window focus detection. Elsewhere it
  falls back to always on top.

The OAuth token store (OS keychain), local database, TLS, desktop notifications, and
update mechanism all use cross-platform back ends.

## Installation

Requirements:

- A recent Rust toolchain (stable), installable from https://rustup.rs.
- An EVE Online account.

Build and run from source:

```
git clone <this-repository>
cd eve-spai
cargo run --release
```

The optimised binary is produced at `target/release/eve-spai`.

### First run

1. Static data. On first launch the application downloads CCP's public Static Data
   Export plus a small set of reference dumps and bakes them into a local SQLite
   database. This happens in the background and is a one-time step per data version.

2. EVE Single Sign-On. Open Settings and log in. The application uses an OAuth2 PKCE
   public client with a loopback callback. A default client id is provided; you can
   substitute your own EVE application client id and callback in Settings if you
   prefer. Access tokens are stored in your operating system keychain, never on disk in
   plain text. The requested scopes cover character location and online status, current
   ship, contacts, and writing autopilot waypoints and fittings.

3. Chat logs. The EVE chat-log directory is auto-detected from the usual locations,
   including Steam Proton prefixes on Linux. You can override the path in Settings.
   Add the intel channels you want to watch.

Application data (the local database and settings) is stored in your platform's
standard application data directory.

## Features

### Intel

- Live parsing of EVE chat logs. As intel is posted it is read, classified, and shown
  as cards with the systems, pilots, ships, and gates involved.
- Pilot recognition against ESI, including names that arrive glued together in plain
  text. Sub-spans are resolved and the longest real name is preferred, so multi-pilot
  reports are split correctly and non-names are dropped.
- Ship recognition by full name, nickname, abbreviation, and localised name, with
  drag-and-drop d-scan formats and multi-word hull handling.
- Keyword detection for the common intel vocabulary (clear, no visual, spike, gate
  camp, bubble, cyno, capital tackled, kill, wormhole, ESS, skyhook, and a call for
  help), with localised kill keywords for non-English clients.
- Per-condition severity colouring and a card filter by type, distance from your
  active character, and free text.
- Kill and battle-report context. Killmail mentions resolve through zKillboard and ESI
  into a badge showing the victim and the dominant alliance on each side, with a
  dedicated kill window.

### Map

- Interactive star map with universe, region, 2D, and 3D jump-distance layouts, plus
  navigation history.
- Live overlays for sovereignty (by alliance or coalition), activity heat (kills,
  jumps), system security, average activity defence multiplier as a backdrop, sov
  upgrades, jump bridges, and jump range.
- Wormhole overlays. Known k-space connections are drawn, and the public hubs Thera and
  Turnur are placed and marked, with their connections shown and individually
  toggleable.
- Your characters on the map. The active character is highlighted and your other
  characters appear as markers, with optional pop-out map windows centred on each.
- Pop-out and overlay modes, including a translucent click-through overlay for use
  beside the game.
- System tooltips and a system information window with security, activity versus the
  regional average, rat profile, sov, and a wormhole section.

### Alerts

- Configurable alert rules combining conditions, distance, and gang size, with actions
  to notify, suppress, or push.
- A dedicated alert window that can stay on top, with desktop notifications, synthesised
  alert tones, and combat alerts driven from your own game logs (under attack,
  scrambled).
- Mobile push through Pushover.

### Fleet communications (XMPP)

- An embedded XMPP client for fleet pings and chat, with multi-user chat rooms you can
  join, leave, and auto-rejoin.
- Direct messages with roster-validated recipients, presence, a contact directory and
  private contact list, closeable conversations with preserved history, per-message
  grouping, notification sounds, mutes, and unread badges across the tray, taskbar,
  sidebar, and chat.
- Fleet-ping parsing with a configurable alert ruleset and a quick broadcast composer.

### Wormholes

- Tracking of transient wormhole connections from intel and from EVE-Scout seeding of
  Thera and Turnur, with signature-prefix de-duplication so seeded and reported holes
  merge.
- Lifetimes shown as coarse upper bounds, with drifter holes handled specifically.

### Routing and characters

- Set Destination with an optional wormhole-aware mode that routes through known holes,
  placing a waypoint at each hole entrance.
- Multiple authenticated characters with location polling, a character switcher, and
  per-character map windows.
- Ship information including warp speed in AU per second with role bonuses, resistances,
  and fitting actions.

### Other

- Clipboard d-scan sharing.
- Configuration packs with preset intel-channel sets per coalition.
- A first-run setup wizard, a manual and automatic update checker with self-replace,
  and an optional minimise-to-tray with autostart on Linux.
