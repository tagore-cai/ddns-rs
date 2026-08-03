# DDNS-RS

[![GitHub release](https://img.shields.io/github/release/jeessy2/ddns-rs.svg?logo=github&style=flat-square)](https://github.com/jeessy2/ddns-rs/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-1.85+-orange?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/docker/pulls/jeessy/ddns-rs?logo=docker)](https://hub.docker.com/r/jeessy/ddns-rs)

English | [中文](README.md)

Automatically obtain your public IPv4 or IPv6 address and resolve it to the
corresponding domain name service.

**DDNS-RS** is a **complete Rust rewrite of [DDNS-GO](https://github.com/jeessy2/ddns-go)**: same
config format, web interface and provider ecosystem, but with lower memory
usage and a smaller binary — well suited for resource-constrained devices
such as OpenWrt routers.

- [DDNS-RS](#ddns-rs)
  - [Features](#features)
  - [Usage on systems](#usage-on-systems)
  - [Docker](#docker)
  - [OpenWrt](#openwrt)
  - [CLI options](#cli-options)
  - [Self-update](#self-update)
  - [Web UI](#web-ui)
  - [Development & build](#development--build)
  - [Credits](#credits)

## Features

- Supports Mac, Windows, Linux; ARM, x86, RISC-V architectures
- DNS providers: Aliyun, Aliyun ESA, Tencent Cloud, Dnspod, Cloudflare, Huawei Cloud, Callback,
  Baidu Cloud, Porkbun, GoDaddy, Namecheap, NameSilo, Dynadot, DNSLA, Eranet, Tnethk, Gcore,
  EdgeOne, NS1 Connect, Rainyun, ClouDNS, Dynv6, Spaceship, Vercel, HiPM DNSMgr, TrafficRoute
- Get IP via API / network card / command
- Run as a service (daemon, service management on Linux/Mac)
- Default 5-minute sync interval
- Multiple DNS providers and domains simultaneously
- Multi-level domain support
- Web-based configuration, easy and convenient, `Deny from WAN` enabled by default
- View the latest 50 logs in the web UI
- Webhook notifications
- TTL support
- Custom parameters for some providers (geo-resolution / multiple IPs)
- **Low resource usage**: ~7.7MB release binary, ~2-8MB runtime memory
- **OpenWrt support**: luci-app plugin with ipk/apk packages

> [!NOTE]
> When enabling public access, it is recommended to use a reverse proxy
> (e.g. Nginx) with HTTPS for security.

## Usage on systems

- Download and extract ddns-rs from [Releases](https://github.com/jeessy2/ddns-rs/releases)
- Run directly: `./ddns-rs`
- Install service (optional)
  - Mac/Linux: `sudo ./ddns-rs -s install`
  - Windows (admin cmd): `.\ddns-rs.exe -s install`
- Configure
  - Open browser and visit `http://localhost:9876`
- [Optional] Uninstall service
  - Mac/Linux: `sudo ./ddns-rs -s uninstall`
  - Windows (admin cmd): `.\ddns-rs.exe -s uninstall`

## Docker

```bash
docker run -d --name ddns-rs --restart=always \
  -p 9876:9876 \
  -v /opt/ddns-rs:/root \
  jeessy/ddns-rs
```

- Visit `http://localhost:9876`
- Config file is saved at `.ddns_go_config.yaml` in `/opt/ddns-rs` (compatible with DDNS-GO)

## OpenWrt

DDNS-RS provides a LuCI plugin supporting OpenWrt 24.10 (ipk) and 25.x (apk).

- Download `openwrt-*.tar.gz` from [Releases](https://github.com/jeessy2/ddns-rs/releases)
- Install the package for your architecture:
  `opkg install ddns-rs_*.ipk` / `apk add ddns-rs_*.apk`
- After installing `luci-app-ddns-rs`, configure it in LuCI **Services → DDNS-RS**

> The DDNS-RS package does **not** bundle the binary. After installation, use
> the **Binary** page in LuCI (upload / custom URL / one-click auto-update) to
> install the ddns-rs binary, so updating ddns-rs does not require reinstalling
> the LuCI package.

## CLI options

```
-v                        Show version
-u                        Self-update to the latest version
-l :9876                  Listen address (default :9876)
-f 300                    Update frequency in seconds (default 300)
-c <path>                 Custom config file path
-s install|uninstall|restart   Service management
-noweb                    Disable web service
-skipVerify               Skip certificate verification
-dns 8.8.8.8              Custom DNS server
-resetPassword <pwd>      Reset password
-d                        Run in background (daemon)
-cacheTimes 5             IP cache compare times (default 5)
```

## Self-update

`./ddns-rs -u` detects the latest version from GitHub Releases, downloads and
replaces the current binary.

- Default repo: `jeessy2/ddns-rs`
- Override with the `DDNS_RS_REPO` environment variable

## Web UI

- Login page (initial username/password setup)
- Config page (multiple DNS configs, IPv4/IPv6 source, Webhook, TTL)
- Log page (latest 50 entries)

## Development & build

```bash
# Run all tests
cargo test --workspace

# Release build
cargo build --release --bin ddns-rs

# Binary output
./target/release/ddns-rs
```

### Project layout

```
crates/
├── ddns_rs_core/        Core engine (config, domain parsing, IP, signing, HTTP client)
├── ddns_rs_providers/   All DNS provider implementations
├── ddns_rs_web/         axum web UI & API
└── ddns_rs_cli/         Entry, service management, self-update
luci/                    OpenWrt packaging (ddns-rs script pkg + luci-app-ddns-rs)
.github/workflows/       CI (test / release / openwrt)
```

## Credits

- [jeessy2/ddns-go](https://github.com/jeessy2/ddns-go) — original Go implementation, source of features and config format
- [sirpdboy/luci-app-ddns-go](https://github.com/sirpdboy/luci-app-ddns-go) — OpenWrt LuCI plugin, basis for the luci-app port

## License

[MIT](LICENSE)
