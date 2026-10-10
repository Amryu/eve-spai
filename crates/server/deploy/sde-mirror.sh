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
date -u +%Y-%m-%dT%H:%M:%SZ > "$DEST/updated.txt"
