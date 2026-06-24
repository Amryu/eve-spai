#!/usr/bin/env sh
# EVE Spai installer for Linux and macOS.
#
#   curl -fsSL https://raw.githubusercontent.com/Amryu/eve-spai/main/install.sh | sh
#
# For a PRIVATE repo, export a GitHub token with `repo` scope first:
#   export GITHUB_TOKEN=ghp_xxx
#
# Override the install dir with PREFIX (default: ~/.local/bin).
set -eu

REPO="Amryu/eve-spai"          # <-- set to your owner/repo
PREFIX="${PREFIX:-$HOME/.local/bin}"
API="https://api.github.com/repos/$REPO"

# --- detect platform -------------------------------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
  Linux)  plat="linux" ;;
  Darwin) plat="macos" ;;
  *) echo "Unsupported OS: $os" >&2; exit 1 ;;
esac
case "$arch" in
  x86_64|amd64) a="x86_64" ;;
  arm64|aarch64) a="aarch64" ;;
  *) echo "Unsupported arch: $arch" >&2; exit 1 ;;
esac
# Linux ships x86_64; macOS ships Apple Silicon (arm64) only.
if [ "$plat" = "linux" ] && [ "$a" != "x86_64" ]; then
  echo "No Linux build for $arch yet." >&2; exit 1
fi
if [ "$plat" = "macos" ] && [ "$a" != "aarch64" ]; then
  echo "Only Apple Silicon (arm64) macOS builds are provided." >&2; exit 1
fi
asset="eve-spai-$plat-$a"

auth=""
[ -n "${GITHUB_TOKEN:-}" ] && auth="-H Authorization: token $GITHUB_TOKEN"

echo "Looking up the latest release of $REPO…"
release_json="$(curl -fsSL $auth -H 'Accept: application/vnd.github+json' "$API/releases/latest")"
tag="$(printf '%s' "$release_json" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n1)"
[ -n "$tag" ] || { echo "Could not find a release (private repo? set GITHUB_TOKEN)." >&2; exit 1; }

# Confirm the asset is actually on this release before trying to download it.
printf '%s' "$release_json" | grep -q "\"name\": *\"$asset\"" \
  || { echo "Release $tag has no asset '$asset'." >&2; exit 1; }

tmp="$(mktemp)"
echo "Downloading $asset ($tag)…"
# Predictable public download URL; the auth header is still honoured (and followed
# through the redirect) for private repos.
curl -fSL $auth "https://github.com/$REPO/releases/download/$tag/$asset" -o "$tmp"

mkdir -p "$PREFIX"
chmod +x "$tmp"
mv "$tmp" "$PREFIX/eve-spai"
echo "Installed eve-spai $tag to $PREFIX/eve-spai"

# macOS: clear the quarantine flag so Gatekeeper doesn't block it.
[ "$plat" = "macos" ] && xattr -dr com.apple.quarantine "$PREFIX/eve-spai" 2>/dev/null || true

case ":$PATH:" in
  *":$PREFIX:"*) ;;
  *) echo "Note: $PREFIX is not on your PATH. Add it, e.g.:"
     echo "  echo 'export PATH=\"$PREFIX:\$PATH\"' >> ~/.profile" ;;
esac
echo "Run it with: eve-spai"
