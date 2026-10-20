#!/usr/bin/env bash
# Builds the browser apps (the wormhole map and the fitting simulator) and puts them on the box,
# then restarts the server to load them.
#   crates/server/deploy/web.sh <ssh target>, or SPAI_DEPLOY_BOX set
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
box="${1:-${SPAI_DEPLOY_BOX:?give the ssh target as the first argument or in SPAI_DEPLOY_BOX}}"
( cd "$repo/crates/spai-web" && trunk build --release --public-url /wh/ )
( cd "$repo/crates/spai-sim-web" && trunk build --release --public-url /sim/ )
# Each replaced whole, so files of an older build do not linger. Only the dist dirs: .env lives
# beside them.
upload() {
    local src=$1 name=$2
    tar -C "$src" -cf - . | ssh "$box" "set -e; d=/root/eve-spai/crates/server/deploy; rm -rf \"\$d/$name.new\"; mkdir \"\$d/$name.new\"; tar -x -C \"\$d/$name.new\"; rm -rf \"\$d/$name\"; mv \"\$d/$name.new\" \"\$d/$name\"; chmod -R a+rX \"\$d/$name\""
}
upload "$repo/crates/spai-web/dist" wh-dist
upload "$repo/crates/spai-sim-web/dist" sim-dist
ssh "$box" 'docker restart eve-spai-br >/dev/null'
echo "deployed; https://eve-spai.com/wh/ and https://eve-spai.com/sim/"
