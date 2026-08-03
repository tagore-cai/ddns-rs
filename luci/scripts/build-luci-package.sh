#!/bin/sh

set -eu

# Build ddns-rs OpenWrt packages (ipk + apk) without the OpenWrt SDK.
# Produces:
#   dist/ddns-rs_{version}-r1_all.{ipk,apk}           (init + config + binary manager)
#   dist/luci-app-ddns-rs_{version}-r1_all.{ipk,apk}  (LuCI frontend + rpcd)
#   dist/sha256sums.txt
#
# Based on the packaging approach of luci-app-oxidns (Sven Shi).

VERSION="${1:-0.1.0}"
OUT_DIR="${2:-dist}"
PKG_VERSION="$(printf '%s' "$VERSION" | sed 's/^v//')"

DDNS_PKG="ddns-rs"
DDNS_BASE="${DDNS_PKG}_${PKG_VERSION}-r1_all"
LUCI_PKG="luci-app-ddns-rs"
LUCI_BASE="${LUCI_PKG}_${PKG_VERSION}-r1_all"

# Paths to package source trees (relative to repo root)
DDNS_DIR="luci/ddns-rs"
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

# ---------------------------------------------------------------- ddns-rs pkg

DDNS_CONTROL_DIR="$TMP_DIR/ddns-control"
DDNS_DATA_DIR="$TMP_DIR/ddns-data"
DDNS_APK_CONTROL_DIR="$TMP_DIR/ddns-apk-control"
mkdir -p "$DDNS_CONTROL_DIR" "$DDNS_DATA_DIR" "$DDNS_APK_CONTROL_DIR" "$OUT_DIR"
OUT_DIR="$(cd "$OUT_DIR" && pwd)"

cat > "$DDNS_CONTROL_DIR/control" <<EOF
Package: $DDNS_PKG
Version: $PKG_VERSION-r1
Architecture: all
Maintainer: ddns-rs maintainers
Depends: ca-bundle, curl
Source: https://github.com/jeessy2/ddns-rs
Section: net
Priority: optional
Description: DDNS-rs client scripts (binary installed via LuCI Binary page)
EOF

mkdir -p "$DDNS_DATA_DIR/etc/init.d" "$DDNS_DATA_DIR/etc/config" "$DDNS_DATA_DIR/usr/libexec"
cp "$DDNS_DIR/files/ddns-rs.init" "$DDNS_DATA_DIR/etc/init.d/ddns-rs"
cp "$DDNS_DIR/files/ddns-rs.conf" "$DDNS_DATA_DIR/etc/config/ddns-rs"
cp "$DDNS_DIR/files/ddns-rs-binary" "$DDNS_DATA_DIR/usr/libexec/ddns-rs-binary"
chmod 755 "$DDNS_DATA_DIR/etc/init.d/ddns-rs" "$DDNS_DATA_DIR/usr/libexec/ddns-rs-binary"

tar_create_gz "$TMP_DIR/ddns-control.tar.gz" -C "$DDNS_CONTROL_DIR" .
tar_create_gz "$TMP_DIR/ddns-data.tar.gz" -C "$DDNS_DATA_DIR" .
create_ipk "$OUT_DIR/${DDNS_BASE}.ipk" "$TMP_DIR/ddns-control.tar.gz" "$TMP_DIR/ddns-data.tar.gz"

cat > "$DDNS_APK_CONTROL_DIR/.PKGINFO" <<EOF
pkgname = $DDNS_PKG
pkgver = $PKG_VERSION-r1
pkgdesc = DDNS-rs client scripts
url = https://github.com/jeessy2/ddns-rs
builddate = $(date +%s)
packager = ddns-rs maintainers
size = $(installed_size "$DDNS_DATA_DIR")
arch = noarch
origin = $DDNS_PKG
license = MIT
depend = ca-bundle
depend = curl
EOF
write_rpcd_restart_script "$DDNS_APK_CONTROL_DIR/.post-install"
cp "$DDNS_APK_CONTROL_DIR/.post-install" "$DDNS_APK_CONTROL_DIR/.post-upgrade"
cp "$DDNS_APK_CONTROL_DIR/.post-install" "$DDNS_APK_CONTROL_DIR/.post-deinstall"

create_apk "$OUT_DIR/${DDNS_BASE}.apk" "$DDNS_APK_CONTROL_DIR" "$DDNS_DATA_DIR"

printf 'Wrote %s\n' "$OUT_DIR/${DDNS_BASE}.ipk"
printf 'Wrote %s\n' "$OUT_DIR/${DDNS_BASE}.apk"

# ------------------------------------------------------------ luci-app pkg

LUCI_CONTROL_DIR="$TMP_DIR/luci-control"
LUCI_DATA_DIR="$TMP_DIR/luci-data"
LUCI_APK_CONTROL_DIR="$TMP_DIR/luci-apk-control"
mkdir -p "$LUCI_CONTROL_DIR" "$LUCI_DATA_DIR" "$LUCI_APK_CONTROL_DIR"

cat > "$LUCI_CONTROL_DIR/control" <<EOF
Package: $LUCI_PKG
Version: $PKG_VERSION-r1
Architecture: all
Maintainer: ddns-rs maintainers
Depends: luci-base, rpcd, ddns-rs
Source: https://github.com/jeessy2/ddns-rs
Section: luci
Priority: optional
Description: LuCI support for DDNS-rs
EOF

mkdir -p "$LUCI_DATA_DIR/www"
if [ -d "$LUCI_DIR/htdocs" ]; then
	cp -R "$LUCI_DIR/htdocs/." "$LUCI_DATA_DIR/www/"
fi
if [ -d "$LUCI_DIR/root" ]; then
	cp -R "$LUCI_DIR/root/." "$LUCI_DATA_DIR/"
fi
chmod 755 "$LUCI_DATA_DIR/usr/libexec/ddns-rs-call" 2>/dev/null || true
if [ -f "$LUCI_DATA_DIR/etc/uci-defaults/99-luci-ddns-rs" ]; then
	chmod 755 "$LUCI_DATA_DIR/etc/uci-defaults/99-luci-ddns-rs"
fi

write_rpcd_restart_script "$LUCI_CONTROL_DIR/postinst"
write_rpcd_restart_script "$LUCI_CONTROL_DIR/postrm"

tar_create_gz "$TMP_DIR/luci-control.tar.gz" -C "$LUCI_CONTROL_DIR" .
tar_create_gz "$TMP_DIR/luci-data.tar.gz" -C "$LUCI_DATA_DIR" .
create_ipk "$OUT_DIR/${LUCI_BASE}.ipk" "$TMP_DIR/luci-control.tar.gz" "$TMP_DIR/luci-data.tar.gz"

cat > "$LUCI_APK_CONTROL_DIR/.PKGINFO" <<EOF
pkgname = $LUCI_PKG
pkgver = $PKG_VERSION-r1
pkgdesc = LuCI support for DDNS-rs
url = https://github.com/jeessy2/ddns-rs
builddate = $(date +%s)
packager = ddns-rs maintainers
size = $(installed_size "$LUCI_DATA_DIR")
arch = noarch
origin = $LUCI_PKG
license = MIT
depend = luci-base
depend = rpcd
depend = ddns-rs
EOF
write_rpcd_restart_script "$LUCI_APK_CONTROL_DIR/.post-install"
cp "$LUCI_APK_CONTROL_DIR/.post-install" "$LUCI_APK_CONTROL_DIR/.post-upgrade"
cp "$LUCI_APK_CONTROL_DIR/.post-install" "$LUCI_APK_CONTROL_DIR/.post-deinstall"

create_apk "$OUT_DIR/${LUCI_BASE}.apk" "$LUCI_APK_CONTROL_DIR" "$LUCI_DATA_DIR"

printf 'Wrote %s\n' "$OUT_DIR/${LUCI_BASE}.ipk"
printf 'Wrote %s\n' "$OUT_DIR/${LUCI_BASE}.apk"

(
	cd "$OUT_DIR"
	sha256sum "${DDNS_BASE}.ipk" "${DDNS_BASE}.apk" "${LUCI_BASE}.ipk" "${LUCI_BASE}.apk"
) > "$OUT_DIR/sha256sums.txt"

printf 'Done. Packages in %s\n' "$OUT_DIR"
