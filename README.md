# EVE Spai

EVE Spai is an intel, map and wormhole tool for EVE Online, built by and for the Imperium.
It reads your intel channels as reports land, puts hostiles on a star map, keeps your
wormhole chains, sizes up a pasted local, turns nearby kills into battle reports and keeps
fleet pings in one window next to the game. Most of it works for any capsuleer; the Jabber
pings follow the Imperium's ping format.

It only uses EVE's public static data (system names, the map, ship and wormhole
reference data), and you sign in with EVE's official Single Sign-On.

Website and screenshots: https://eve-spai.com

## What you can do with it

- See where things stand on the Dashboard: every character's surroundings, the fleets
  forming up, the map around you, recent battles and what you missed while away, as tiles
  you pick and drag into place.
- Watch your intel channels and read reports as cards, with the systems, pilots and
  ships worked out and the jumps from you counted over gates and your jump bridges.
- Get alerts you design, by what was said, how close it is, how big the gang is, or which
  tagged pilots and systems are involved, with sound, desktop notifications, an alert
  window over the game and phone push.
- See hostiles on an interactive star map with sovereignty, activity, jump bridge, camp and
  wormhole layers, plan gate, capital jump and titan bridge routes, and send them to the
  game as waypoints. A see-through copy of the map can sit over EVE.
- Keep your wormhole chains on one map, logged as your characters jump, with probe scan
  signatures per system, scan routes for your scouts, Thera and Turnur from EVE-Scout,
  and end-to-end encrypted sharing with your group, also as a web map in the browser.
- Paste local and size up every pilot: danger, kills and losses, gang size, habits,
  standings and your own notes.
- Get battle reports built from the kills near you, adjust the sides and share them as a
  link on eve-spai.com.
- Read and send Jabber chat and fleet pings without leaving the app, with ping rules and a
  ping window over the game.
- Keep notes and tags on systems and pilots, organised in folders you can export and share.
- Ask the optional AI assistant about the situation, typed or by push to talk. It works
  with Claude, OpenAI, Gemini, a local model or your Claude or ChatGPT subscription, reads
  only what you allow and asks before it acts.
- Follow intel, the map and routes from a phone or a second screen through the local web
  view.
- Use it in English, Deutsch, Español, Français, Русский or 中文.

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

The first time you open EVE Spai, a short setup walks you through the basics: your
language, your EVE chat-log folder (found on its own, including Steam and Proton on Linux),
the intel channels to watch, signing in your character with EVE Single Sign-On and a
theme. Picking the Imperium channel pack adds steps for the jump bridge network, sov upgrades
and Jabber.
Everything can be changed later in Settings, where the setup can be run again.

Meanwhile EVE's public reference data downloads once in the background and stays on your
machine; the map and ship lookups fill in as it finishes. Your sign-in is kept in your
system's secure keychain, never in a plain file. It lets the app see where your characters
are and set autopilot waypoints for you.

## Platforms

EVE Spai is developed on Linux and published for Linux, Windows and macOS. Sound, the
system tray and the alert overlay work on all three. A few window helpers, such as noticing
whether the EVE client has focus, are Linux-only and stay quiet elsewhere.

On Linux the fleet dashboard sign-in runs in a small helper, `eve-spai-fleet-login`, which
the install script puts next to the app. It needs WebKitGTK (webkit2gtk-4.1); everything
else works without it.

## How it is built

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).
