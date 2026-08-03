# SPDX-License-Identifier: MIT
#
# luci-app-ddns-rs: LuCI frontend + service for ddns-rs.
#
# Single architecture-independent package (PKGARCH=all) containing the LuCI
# UI, rpcd backend, init script, UCI config and the ddns-rs binary manager.
# The ddns-rs binary itself is NOT bundled - it is installed and updated via
# the LuCI "Binary" page (usr/libexec/ddns-rs-binary) or `ddns-rs -u`.
#

include $(TOPDIR)/rules.mk

PKG_NAME:=luci-app-ddns-rs
PKG_VERSION:=0.1.0
PKG_RELEASE:=1

PKG_LICENSE:=MIT
PKG_MAINTAINER:=ddns-rs maintainers

LUCI_TITLE:=LuCI support for ddns-rs (Rust DDNS client)
LUCI_DEPENDS:=+luci-base +rpcd +curl +ca-bundle
LUCI_PKGARCH:=all

define Package/luci-app-ddns-rs/conffiles
/etc/config/ddns-rs
/etc/ddns-rs/ddns-rs-config.yaml
endef

define Package/luci-app-ddns-rs/postinst
#!/bin/sh
[ -n "$${IPKG_INSTROOT:-}" ] && exit 0
rm -f /tmp/luci-indexcache* 2>/dev/null || true
rm -rf /tmp/luci-modulecache/* 2>/dev/null || true
if [ -d /www/luci-static/resources/view/ddns-rs ]; then
	find /www/luci-static/resources/view/ddns-rs -type f -name '*.js' -exec touch {} + 2>/dev/null || true
fi
if [ -x /etc/init.d/rpcd ]; then
	/etc/init.d/rpcd restart >/dev/null 2>&1 || true
fi
exit 0
endef

define Package/luci-app-ddns-rs/postrm
#!/bin/sh
[ -n "$${IPKG_INSTROOT:-}" ] && exit 0
rm -f /tmp/luci-indexcache* 2>/dev/null || true
rm -rf /tmp/luci-modulecache/* 2>/dev/null || true
if [ -x /etc/init.d/rpcd ]; then
	/etc/init.d/rpcd restart >/dev/null 2>&1 || true
fi
exit 0
endef

include $(TOPDIR)/feeds/luci/luci.mk

# call BuildPackage - OpenWrt buildroot signature
