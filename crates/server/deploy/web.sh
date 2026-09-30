#!/usr/bin/env bash
# Builds the web wormhole map and puts it on the box, then restarts the server to load it.
#   crates/server/deploy/web.sh [ssh target]
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
box="${1:-root@100.68.81.80}"
( cd "$repo/crates/spai-web" && trunk build --release --public-url /wh/ )
# Replaced whole, so files of an older build do not linger. Only wh-dist: .env lives beside it.
tar -C "$repo/crates/spai-web/dist" -cf - . | ssh "$box" 'set -e; d=/root/eve-spai/crates/server/deploy; rm -rf "$d/wh-dist.new"; mkdir "$d/wh-dist.new"; tar -x -C "$d/wh-dist.new"; rm -rf "$d/wh-dist"; mv "$d/wh-dist.new" "$d/wh-dist"; chmod -R a+rX "$d/wh-dist"; docker restart eve-spai-br >/dev/null'
echo "deployed; https://eve-spai.com/wh/"
