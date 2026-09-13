#!/usr/bin/env bash
# Screenshots the web view's fixture demo, the way `uitest_screenshots` renders the egui scenes.
#
# The egui harness cannot render HTML, so this is what stands in for it (see
# ui-tickets/GAP-011-web-render-unreachable/). It shoots the demo server, which serves fixtures on
# loopback and never touches the live profile, so the PNGs are safe to commit.
#
#   app/src/uitest/webshot.sh [outdir] [width,height ...]
#
# Defaults to target/webshots at desktop and phone sizes.
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
out="${1:-$repo/target/webshots}"
shift || true
sizes=("$@")
[ ${#sizes[@]} -eq 0 ] && sizes=(1440,900 390,844)

url="${SPAI_WEBSHOT_URL:-http://127.0.0.1:6799/?t=demo}"

# The flatpak Firefox cannot see /tmp or an arbitrary profile path: its only writable host
# filesystem is xdg-download. Both the scratch profile and the PNGs therefore live under
# ~/Downloads and get copied out, which is the whole reason an obvious `--screenshot /tmp/x.png`
# silently produces nothing.
stage="$HOME/Downloads/.spai-webshot"
# Assets are served with an ETag keyed on the crate version, so a profile kept between runs answers
# from cache and shoots the previous build's CSS and JS. Every run starts from an empty profile.
rm -rf "$stage/profile"
mkdir -p "$stage/profile" "$out"

# A server left over from an earlier run serves that run's assets, so a CSS or JS change screenshots
# as if it had never been made. Reuse is not worth the hours that costs: always start a fresh one.
fuser -k 6799/tcp >/dev/null 2>&1 || true
demo_pid=""
if true; then
  echo "starting the demo server"
  ( cd "$repo" && exec cargo test --bin eve-spai webdemo -- --ignored --nocapture >"$stage/demo.log" 2>&1 ) &
  demo_pid=$!
  for _ in $(seq 1 60); do
    curl -sf --max-time 1 http://127.0.0.1:6799/healthz >/dev/null && break
    sleep 1
  done
fi
curl -sf --max-time 2 http://127.0.0.1:6799/healthz >/dev/null || { echo "demo server never came up; see $stage/demo.log" >&2; exit 1; }

for size in "${sizes[@]}"; do
  name="${size%%,*}"
  echo "shooting ${size}"
  # Cleared first: a shot that fails leaves the previous run's PNG here, and the copy below would
  # then hand back an image of the wrong page with no error anywhere.
  rm -f "$stage/w-$name.png"
  # --no-remote plus its own profile: without them a running Firefox swallows the URL and exits 0
  # having screenshotted nothing.
  timeout 120 flatpak run --command=firefox org.mozilla.firefox \
    --headless --no-remote --profile "$stage/profile" \
    --window-size="$size" --screenshot "$stage/w-$name.png" "$url" >/dev/null 2>&1 || true
  if [ -s "$stage/w-$name.png" ]; then
    cp "$stage/w-$name.png" "$out/web-${name}.png"
  else
    echo "no PNG produced at $size" >&2
  fi
done

# Stop it by the port it holds, never by a command-line pattern. `cargo test` execs the test binary
# as a child, so killing the pid we started leaves the server running; and a `pkill -f` broad enough
# to catch that binary is also broad enough to catch the shell running this script, which is how
# this exited 144 twice while it was being written.
if [ -n "$demo_pid" ]; then
  fuser -k 6799/tcp >/dev/null 2>&1 || true
  wait "$demo_pid" 2>/dev/null || true
fi

ls -la "$out"
