# SPDX-License-Identifier: GPL-3.0-only
#
# ddns-rs: Rust rewrite of ddns-go, packaged for OpenWrt.
#
# This package contains ONLY the init script and configuration - it does NOT
# bundle the ddns-rs binary. The binary is managed through the LuCI interface
# (upload / custom URL / auto-update via ddns-rs -u), which keeps this package
# architecture-independent (PKGARCH=all) and easy to update without reinstalling.
#

include $(TOPDIR)/rules.mk

PKG_NAME:=ddns-rs
PKG_VERSION:=0.1.0
PKG_RELEASE:=1

PKG_LICENSE:=MIT
PKG_MAINTAINER:=ddns-rs maintainers
PKG_ARCH:=all

include $(INCLUDE_DIR)/package.mk

define Package/ddns-rs
  SECTION:=net
  CATEGORY:=Network
  SUBMENU:=IP Addresses and Names
  TITLE:=DDNS-rs - Rust DDNS client (IPv4/IPv6)
  URL:=https://github.com/jeessy2/ddns-rs
  DEPENDS:=+ca-bundle +curl
  USERID:=ddns-rs:ddns-rs
endef

define Package/ddns-rs/description
  A simple and easy-to-use IP binding domain name tool (Rust port of ddns-go),
  supports IPv4 and IPv6. The binary is NOT bundled; install/manage it via
  the LuCI interface (ddns-rs-binary) or ddns-rs -u self-update.
endef

define Package/ddns-rs/conffiles
/etc/config/ddns-rs
/etc/ddns-rs/ddns-rs-config.yaml
endef

define Package/ddns-rs/install
	$(INSTALL_DIR) $(1)/etc/init.d
	$(INSTALL_DIR) $(1)/etc/config
	$(INSTALL_DIR) $(1)/etc/ddns-rs
	$(INSTALL_DIR) $(1)/usr/libexec

	$(INSTALL_BIN) $(CURDIR)/files/ddns-rs.init $(1)/etc/init.d/ddns-rs
	$(INSTALL_BIN) $(CURDIR)/files/ddns-rs-binary $(1)/usr/libexec/ddns-rs-binary
	$(INSTALL_CONF) $(CURDIR)/files/ddns-rs.conf $(1)/etc/config/ddns-rs
endef

$(eval $(call BuildPackage,ddns-rs))
