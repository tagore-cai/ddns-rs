#!/bin/sh

set -eu

# Build the ddns-rs OpenWrt LuCI package (.ipk for opkg / .apk for the
# OpenWrt 25.x apk package manager) without the OpenWrt SDK.
#
# Produces a single architecture-independent package that contains both the
# ddns-rs service files (init script, config, binary manager) and the LuCI
# frontend. The ddns-rs binary itself is NOT bundled; it is installed and
# updated via the LuCI "Binary" page (usr/libexec/ddns-rs-binary).
#
#   dist/luci-app-ddns-rs_{version}-r1_all.{ipk,apk}
#   dist/sha256sums.txt
#
# Based on the packaging approach of luci-app-oxidns (Sven Shi).

VERSION="${1:-0.1.0}"
OUT_DIR="${2:-dist}"
PKG_VERSION="$(printf '%s' "$VERSION" | sed 's/^v//')"

LUCI_PKG="luci-app-ddns-rs"
LUCI_BASE="${LUCI_PKG}_${PKG_VERSION}-r1_all"

# Path to the package source tree (relative to repo root)
LUCI_DIR="luci/luci-app-ddns-rs"

# Resolve the directory holding the helper node scripts (repo-root/luci/scripts)
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

need_cmd() {
	command -v "$1" >/dev/null 2>&1 || {
		printf 'required command not found: %s\n' "$1" >&2
		exit 1
	}
}

need_cmd tar
need_cmd gzip
need_cmd node
need_cmd sha256sum

export COPYFILE_DISABLE=1

tar_create_gz() {
	tar_gz_out="$1"
	shift
	tar --format=ustar --owner=0 --group=0 --numeric-owner -czf "$tar_gz_out" "$@"
}

write_file_list() {
	file_list_dir="$1"
	file_list_out="$2"
	(
		cd "$file_list_dir"
		find . ! -name . | sed 's#^\./##' | LC_ALL=C sort
	) > "$file_list_out"
}

tar_create_segment_gz_from_list() {
	tar_segment_out="$1"
	tar_segment_dir="$2"
	tar_segment_list="$3"
	tar_segment_raw="${tar_segment_out%.gz}"
	tar_segment_cut="$tar_segment_raw.cut"

	tar --format=ustar --owner=0 --group=0 --numeric-owner -cf "$tar_segment_raw" -C "$tar_segment_dir" -T "$tar_segment_list"
	node "$SCRIPT_DIR/strip-tar-eof.mjs" "$tar_segment_raw" "$tar_segment_cut"
	gzip -9n < "$tar_segment_cut" > "$tar_segment_out"
}

installed_size() {
	find "$1" -type f -exec wc -c {} + | awk 'END { print $1 + 0 }'
}

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/ddns-rs-build.XXXXXX")"
cleanup() {
	rm -rf "$TMP_DIR"
}
trap cleanup EXIT HUP INT TERM

create_ipk() {
	ipk_out="$1"
	ipk_control_tar="$2"
	ipk_data_tar="$3"
	ipk_dir="$TMP_DIR/ipk-$(basename "$ipk_out" .ipk)"

	rm -rf "$ipk_dir"
	mkdir -p "$ipk_dir"
	printf '2.0\n' > "$ipk_dir/debian-binary"
	cp "$ipk_data_tar" "$ipk_dir/data.tar.gz"
	cp "$ipk_control_tar" "$ipk_dir/control.tar.gz"
	tar_create_gz "$ipk_out" -C "$ipk_dir" debian-binary control.tar.gz data.tar.gz
}

create_apk() {
	apk_out="$1"
	apk_control_dir="$2"
	apk_data_dir="$3"
	apk_name="$(basename "$apk_out" .apk)"
	apk_control_list="$TMP_DIR/$apk_name.control.list"
	apk_data_raw="$TMP_DIR/$apk_name.data.tar"
	apk_data_tar="$TMP_DIR/$apk_name.data.tar.gz"
	apk_control_tar="$TMP_DIR/$apk_name.control.tar.gz"

	node "$SCRIPT_DIR/write-apk-data-tar.mjs" "$apk_data_dir" "$apk_data_raw"
	gzip -9n < "$apk_data_raw" > "$apk_data_tar"
	apk_datahash="$(sha256sum "$apk_data_tar" | awk '{ print $1 }')"
	printf 'datahash = %s\n' "$apk_datahash" >> "$apk_control_dir/.PKGINFO"

	{
		printf '.PKGINFO\n'
		(
			cd "$apk_control_dir"
			find . ! -name . ! -name .PKGINFO | sed 's#^\./##' | LC_ALL=C sort
		)
	} > "$apk_control_list"

	tar_create_segment_gz_from_list "$apk_control_tar" "$apk_control_dir" "$apk_control_list"
	cat "$apk_control_tar" "$apk_data_tar" > "$apk_out"
}

write_rpcd_restart_script() {
	out="$1"
	cat > "$out" <<'EOF'
#!/bin/sh
[ -n "${IPKG_INSTROOT:-}" ] && exit 0
rm -f /tmp/luci-indexcache* 2>/dev/null || true
rm -rf /tmp/luci-modulecache/* 2>/dev/null || true
if [ -d /www/luci-static/resources/view/ddns-rs ]; then
	find /www/luci-static/resources/view/ddns-rs -type f -name '*.js' -exec touch {} + 2>/dev/null || true
fi
if [ -x /etc/init.d/rpcd ]; then
	/etc/init.d/rpcd restart >/dev/null 2>&1 || true
fi
exit 0
EOF
	chmod 755 "$out"
}

CONTROL_DIR="$TMP_DIR/control"
DATA_DIR="$TMP_DIR/data"
APK_CONTROL_DIR="$TMP_DIR/apk-control"
mkdir -p "$CONTROL_DIR" "$DATA_DIR" "$APK_CONTROL_DIR" "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

cat > "$CONTROL_DIR/control" <<EOF
Package: $LUCI_PKG
Version: $PKG_VERSION-r1
Architecture: all
Maintainer: ddns-rs maintainers
Depends: luci-base, rpcd, curl, ca-bundle
Source: https://github.com/jeessy2/ddns-rs
Section: luci
Priority: optional
Description: LuCI support for DDNS-rs (service, config and binary manager)
EOF

mkdir -p "$DATA_DIR/www"
if [ -d "$LUCI_DIR/htdocs" ]; then
	cp -R "$LUCI_DIR/htdocs/." "$DATA_DIR/www/"
fi
if [ -d "$LUCI_DIR/root" ]; then
	cp -R "$LUCI_DIR/root/." "$DATA_DIR/"
fi
chmod 755 "$DATA_DIR/etc/init.d/ddns-rs" 2>/dev/null || true
chmod 755 "$DATA_DIR/usr/libexec/ddns-rs-binary" 2>/dev/null || true
chmod 755 "$DATA_DIR/usr/libexec/ddns-rs-call" 2>/dev/null || true
if [ -f "$DATA_DIR/etc/uci-defaults/99-luci-ddns-rs" ]; then
	chmod 755 "$DATA_DIR/etc/uci-defaults/99-luci-ddns-rs"
fi

write_rpcd_restart_script "$CONTROL_DIR/postinst"
write_rpcd_restart_script "$CONTROL_DIR/postrm"

tar_create_gz "$TMP_DIR/control.tar.gz" -C "$CONTROL_DIR" .
tar_create_gz "$TMP_DIR/data.tar.gz" -C "$DATA_DIR" .
create_ipk "$OUT_DIR/${LUCI_BASE}.ipk" "$TMP_DIR/control.tar.gz" "$TMP_DIR/data.tar.gz"

cat > "$APK_CONTROL_DIR/.PKGINFO" <<EOF
pkgname = $LUCI_PKG
pkgver = $PKG_VERSION-r1
pkgdesc = LuCI support for DDNS-rs (service, config and binary manager)
url = https://github.com/jeessy2/ddns-rs
builddate = $(date +%s)
packager = ddns-rs maintainers
size = $(installed_size "$DATA_DIR")
arch = noarch
origin = $LUCI_PKG
license = MIT
depend = luci-base
depend = rpcd
depend = curl
depend = ca-bundle
EOF
write_rpcd_restart_script "$APK_CONTROL_DIR/.post-install"
cp "$APK_CONTROL_DIR/.post-install" "$APK_CONTROL_DIR/.post-upgrade"
cp "$APK_CONTROL_DIR/.post-install" "$APK_CONTROL_DIR/.post-deinstall"

create_apk "$OUT_DIR/${LUCI_BASE}.apk" "$APK_CONTROL_DIR" "$DATA_DIR"

printf 'Wrote %s\n' "$OUT_DIR/${LUCI_BASE}.ipk"
printf 'Wrote %s\n' "$OUT_DIR/${LUCI_BASE}.apk"

(
	cd "$OUT_DIR"
	sha256sum "${LUCI_BASE}.ipk" "${LUCI_BASE}.apk"
) > "$OUT_DIR/sha256sums.txt"

printf 'Done. Packages in %s\n' "$OUT_DIR"
