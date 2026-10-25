#!/bin/sh
# Mirrors the newest release's downloads for pilots GitHub serves slowly:
#   eve-spai.com/windows/current, /macos/current and /linux/current,
# each with a .json beside it naming its version and checksum. The app checks every download against
# GitHub's own checksum, so the mirror only has to be fast, not trusted.
#
# Run every 10 minutes (cron or a systemd timer) on the box that serves eve-spai.com, so the mirror
# is never more than about ten minutes behind a release:
#   */10 * * * * /srv/eve-spai/release-mirror.sh /srv/nginx/www/eve-spai
# A file already at the newest version is left alone, and each new one is fetched to a temporary name
# and moved into place only when complete, its .json after it; a failed fetch keeps the previous
# copy and is tried again on the next run.
set -eu

DEST=${1:-/srv/nginx/www/eve-spai}
REPO=Amryu/eve-spai

# Two runs at once would fetch the same files twice.
exec 9>"${TMPDIR:-/tmp}/release-mirror.lock"
flock -n 9 || exit 0

# The newest release's tag, from where GitHub's "latest" page redirects to. No API, no rate limit.
tag=$(curl -fsSI --retry 3 --max-time 60 "https://github.com/$REPO/releases/latest" \
    | tr -d '\r' | sed -n 's#^[Ll]ocation: .*/releases/tag/\(.*\)$#\1#p' | tail -n 1)
if [ -z "$tag" ]; then
    echo "release-mirror: could not find the newest release" >&2
    exit 1
fi
version=${tag#v}

mirror() {
    asset=$1
    path=$2
    out="$DEST/$path"
    mkdir -p "$(dirname "$out")"
    if grep -q "\"version\":\"$version\"" "$out.json" 2>/dev/null && [ -s "$out" ]; then
        return 0
    fi
    tmp="$out.part"
    if curl -fsSL --retry 3 --max-time 1800 -o "$tmp" "https://github.com/$REPO/releases/download/$tag/$asset" && [ -s "$tmp" ]; then
        sum=$(sha256sum "$tmp" | cut -d' ' -f1)
        size=$(wc -c < "$tmp" | tr -d ' ')
        chmod 644 "$tmp"
        mv -f "$tmp" "$out"
        printf '{"version":"%s","sha256":"%s","size":%s,"name":"%s","updated":"%s"}\n' \
            "$version" "$sum" "$size" "$asset" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" > "$out.json.part"
        chmod 644 "$out.json.part"
        mv -f "$out.json.part" "$out.json"
        echo "release-mirror: $path is now v$version"
    else
        rm -f "$tmp"
        echo "release-mirror: $asset of $tag failed, keeping the previous copy" >&2
    fi
}

mirror eve-spai-windows-x86_64.exe windows/current
mirror eve-spai-macos-aarch64 macos/current
mirror eve-spai-linux-x86_64 linux/current
date -u +%Y-%m-%dT%H:%M:%SZ > "$DEST/release-mirror-checked.txt"
