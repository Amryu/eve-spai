#!/bin/sh
# Mirrors the static data the app builds its map from, so pilots far from the sources (China, where
# fuzzwork.co.uk crawls at nothing) download it from eve-spai.com behind Cloudflare instead.
#
# Run daily (cron or a systemd timer) on the box that serves eve-spai.com:
#   /srv/eve-spai/sde-mirror.sh /srv/nginx/www/eve-spai/sde
# Each file is fetched to a temporary name and moved into place only when complete, so a half-done
# fetch is never served; a failed fetch leaves yesterday's copy in place.
set -eu

DEST=${1:-/srv/nginx/www/eve-spai/sde}
FUZZ=https://www.fuzzwork.co.uk/dump/latest/csv
CCP=https://developers.eveonline.com/static-data/eve-online-static-data-latest-jsonl.zip
mkdir -p "$DEST"

fetch() {
    url=$1
    name=$2
    tmp="$DEST/.$name.part"
    if curl -fsSL --retry 3 --max-time 1800 -o "$tmp" "$url" && [ -s "$tmp" ]; then
        mv -f "$tmp" "$DEST/$name"
    else
        rm -f "$tmp"
        echo "sde-mirror: $name failed, keeping the previous copy" >&2
    fi
}

for f in mapRegions mapConstellations mapSolarSystems mapSolarSystemJumps invGroups invTypes dgmTypeAttributes invTraits; do
    fetch "$FUZZ/$f.csv" "$f.csv"
done
fetch "$CCP" "eve-online-static-data-latest-jsonl.zip"

# The fitting simulator's data, baked from the zip by the server image's `spai-fit-bake`
# (crates/server/Dockerfile): the web app fetches dogma-v*.bin.gz, the desktop falls back to it
# when a profile has no blob of its own. Baked only when the zip changed; a failed bake keeps the
# previous blob.
IMAGE=${SDE_BAKE_IMAGE:-eve-spai-br:latest}
if [ -s "$DEST/eve-online-static-data-latest-jsonl.zip" ]; then
    sum=$(sha256sum "$DEST/eve-online-static-data-latest-jsonl.zip" | cut -d' ' -f1)
    if [ "$sum" != "$(cat "$DEST/.baked-from" 2>/dev/null)" ]; then
        if docker run --rm --user 0 --entrypoint /usr/local/bin/spai-fit-bake -v "$DEST:/sde" "$IMAGE" \
            /sde/eve-online-static-data-latest-jsonl.zip /sde; then
            echo "$sum" > "$DEST/.baked-from"
        else
            echo "sde-mirror: fitting data bake failed, keeping the previous blob" >&2
        fi
    fi
fi
date -u +%Y-%m-%dT%H:%M:%SZ > "$DEST/updated.txt"
