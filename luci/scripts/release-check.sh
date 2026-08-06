#!/bin/sh

set -eu

# Full release pipeline for the ddns-rs LuCI packages:
#   static checks -> integration checks -> build .ipk/.apk -> verify checksums

VERSION="${1:-${VERSION:-0.1.0}}"
OUT_DIR="${2:-${OUT_DIR:-dist}}"
PKG_VERSION="$(printf '%s' "$VERSION" | sed 's/^v//')"

need_cmd() {
	command -v "$1" >/dev/null 2>&1 || {
		printf 'required command not found: %s\n' "$1" >&2
		exit 1
	}
}

need_cmd awk
need_cmd gzip
need_cmd grep
need_cmd node
need_cmd sha256sum
need_cmd tar

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LUCI_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# Build the Vue frontend if it hasn't been built yet (the bundle is a
# build artifact and not committed).
BUNDLE_DIR="$LUCI_DIR/luci-app-ddns-rs/htdocs/luci-static/resources/ddns-rs-app"
if [ ! -f "$BUNDLE_DIR/ddns-rs-app.js" ]; then
	need_cmd npm
	(
		cd "$LUCI_DIR"
		npm ci
		npm run build
	)
fi

"$SCRIPT_DIR/check.sh"
"$SCRIPT_DIR/build-luci-package.sh" "$VERSION" "$OUT_DIR"
PKG_VERSION="${PKG_VERSION}-r1" "$SCRIPT_DIR/integration-check.sh" "$OUT_DIR"

printf 'Release check passed for %s in %s\n' "$VERSION" "$OUT_DIR"
