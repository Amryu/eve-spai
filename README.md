# EVE Spai

EVE Spai is an intel and situational-awareness tool for EVE Online. It reads your
in-game chat logs as intel is posted, puts hostiles on a star map, raises alerts you can
tune, and keeps fleet pings and killboard context in one window next to the game.

It only uses EVE's public static data (system names, the map, ship and wormhole
reference data), and you sign in with EVE's official Single Sign-On.

## What you can do with it

- Watch your intel channels and read reports as cards, with the systems, pilots, and
  ships involved worked out for you.
- See hostiles on an interactive star map, with sovereignty, activity, and wormhole
  overlays.
- Get alerts you design — by what was said, how close it is, and how big the gang is —
  with sound and desktop notifications.
- Look up pilots on zKillboard, dragging in a whole local list at once.
- Read and send fleet pings and chat without leaving the app.

## Installing

The simplest way is the install script for your system. Run it in a terminal.

Linux and macOS:

```
curl -fsSL https://raw.githubusercontent.com/Amryu/eve-spai/main/install.sh | sh
```

Windows (PowerShell):

```
irm https://raw.githubusercontent.com/Amryu/eve-spai/main/install.ps1 | iex
```

The script downloads the latest published build for your system and puts it in place.

### Building it yourself

You can always build from source, which works on any system. Install Rust from
https://rustup.rs, then:

```
git clone https://github.com/Amryu/eve-spai.git
cd eve-spai
cargo run --release
```

The first build takes a few minutes.

## First run

A few things happen the first time you open EVE Spai:

1. Reference data. It downloads EVE's public data once and stores it on your machine.
   This runs in the background; the map and ship lookups fill in as it finishes.

2. Log in. Open Settings and sign in with EVE Single Sign-On — the same login you use
   for the game. Your access is kept in your system's secure keychain, never in a plain
   file. This is what lets the app see where your characters are and set autopilot
   waypoints for you.

3. Chat logs and channels. EVE Spai finds your EVE chat-log folder on its own (including
   Steam and Proton on Linux); you can point it elsewhere in Settings. Add the intel
   channels you want it to watch, and reports start coming in.

That is all you need to get going.

## Platforms

EVE Spai is developed on Linux and runs there best. It is built to work on Windows and
macOS as well; a few extras — alert-sound playback, the system tray, and the
always-on-top overlay — are Linux-first for now and simply stay quiet elsewhere.
