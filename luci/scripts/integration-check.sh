#!/bin/sh

set -eu

# Integration checks for the built ddns-rs LuCI package:
#  verifies the .ipk/.apk archives contain the expected members.

OUT_DIR="${1:-dist}"
PKG_VERSION="${PKG_VERSION:-0.1.0-r1}"
BASE="luci-app-ddns-rs_${PKG_VERSION}_all"

json_ok() {
	node -e "const v=JSON.parse(require('fs').readFileSync(0,'utf8')); if (!($1)) process.exit(1);"
}

tar_has_member() {
	tar -tzf "$1" | awk -v member="$2" '
		{
			path = $0;
			sub(/^\.\//, "", path);
			sub(/\/$/, "", path);
			if (path == member)
				found = 1;
		}
		END { exit found ? 0 : 1 }
	'
}

tar_member_contains() {
	tar -xOzf "$1" "$2" 2>/dev/null | grep -q "$3"
}

apk_data_has_checksum() {
	gzip -dc "$1" | grep -q 'APK-TOOLS.checksum.SHA1='
}

tar_nested_has_member() {
	outer="$1"
	inner="$2"
	member="$3"
	nested="$(mktemp "${TMPDIR:-/tmp}/ddns-rs-nested.XXXXXX")"
	if ! tar -xOf "$outer" "$inner" > "$nested" 2>/dev/null &&
		! tar -xOf "$outer" "./$inner" > "$nested" 2>/dev/null; then
		rm -f "$nested"
		return 1
	fi

	if tar -tzf "$nested" | awk -v member="$member" '
		{
			path = $0;
			sub(/^\.\//, "", path);
			if (path == member)
				found = 1;
		}
		END { exit found ? 0 : 1 }
	'; then
		rm -f "$nested"
		return 0
	fi

	rm -f "$nested"
	return 1
}

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
need_cmd tar

# --- ipk ---
tar_has_member "$OUT_DIR/${BASE}.ipk" debian-binary
tar_has_member "$OUT_DIR/${BASE}.ipk" control.tar.gz
tar_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" control.tar.gz postinst
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" control.tar.gz postrm
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz etc/init.d/ddns-rs
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz etc/config/ddns-rs
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz usr/libexec/ddns-rs-binary
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz usr/libexec/ddns-rs-call
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz usr/share/luci/menu.d/luci-app-ddns-rs.json
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz usr/share/rpcd/acl.d/luci-app-ddns-rs.json
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz usr/share/rpcd/ucode/luci.ddns-rs
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz www/luci-static/resources/view/ddns-rs/config.js
tar_nested_has_member "$OUT_DIR/${BASE}.ipk" data.tar.gz www/luci-static/resources/view/ddns-rs/binary.js

# --- apk ---
tar_has_member "$OUT_DIR/${BASE}.apk" .PKGINFO
tar_member_contains "$OUT_DIR/${BASE}.apk" .PKGINFO '^arch = noarch$'
tar_member_contains "$OUT_DIR/${BASE}.apk" .PKGINFO '^datahash = [0-9a-f][0-9a-f]*$'
apk_data_has_checksum "$OUT_DIR/${BASE}.apk"
tar_has_member "$OUT_DIR/${BASE}.apk" etc/init.d/ddns-rs
tar_has_member "$OUT_DIR/${BASE}.apk" etc/config/ddns-rs
tar_has_member "$OUT_DIR/${BASE}.apk" usr/libexec/ddns-rs-binary
tar_has_member "$OUT_DIR/${BASE}.apk" usr/libexec/ddns-rs-call
tar_has_member "$OUT_DIR/${BASE}.apk" usr/share/luci/menu.d
tar_has_member "$OUT_DIR/${BASE}.apk" www/luci-static/resources/view/ddns-rs
tar_has_member "$OUT_DIR/${BASE}.apk" .post-install
tar_has_member "$OUT_DIR/${BASE}.apk" .post-upgrade
tar_has_member "$OUT_DIR/${BASE}.apk" .post-deinstall

(cd "$OUT_DIR" && sha256sum -c sha256sums.txt)

printf 'Integration check passed for %s\n' "$OUT_DIR"
