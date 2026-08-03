# DDNS-RS

[![GitHub release](https://img.shields.io/github/release/jeessy2/ddns-rs.svg?logo=github&style=flat-square)](https://github.com/jeessy2/ddns-rs/releases/latest)
[![Rust](https://img.shields.io/badge/Rust-1.85+-orange?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Docker](https://img.shields.io/docker/pulls/jeessy/ddns-rs?logo=docker)](https://hub.docker.com/r/jeessy/ddns-rs)

中文 | [English](README_EN.md)

自动获得你的公网 IPv4 或 IPv6 地址，并解析到对应的域名服务。

**DDNS-RS** 是 [DDNS-GO](https://github.com/jeessy2/ddns-go) 的 **Rust 完整重写版**：保持相同的配置格式、Web 界面与服务商生态，但拥有更低的内存占用和更小的二进制体积，更适合路由器（OpenWrt）等资源受限设备。

- [DDNS-RS](#ddns-rs)
    - [特性](#特性)
    - [系统中使用](#系统中使用)
    - [Docker 中使用](#docker-中使用)
    - [OpenWrt 中使用](#openwrt-中使用)
    - [CLI 参数](#cli-参数)
    - [自更新](#自更新)
    - [界面](#界面)
    - [开发与自行编译](#开发与自行编译)
    - [参考项目](#参考项目)

## 特性

- 支持 Mac、Windows、Linux 系统，支持 ARM、x86、RISC-V 架构
- 支持的域名服务商 `阿里云` `阿里云 ESA` `腾讯云` `Dnspod` `Cloudflare` `华为云` `Callback` `百度云` `Porkbun` `GoDaddy` `Namecheap` `NameSilo` `Dynadot` `DNSLA` `Eranet` `Tnethk` `Gcore` `EdgeOne` `NS1 Connect` `雨云` `ClouDNS` `Dynv6` `Spaceship` `Vercel` `HiPM DNSMgr` `TrafficRoute`
- 支持接口/网卡/命令获取 IP
- 支持以服务的方式运行（Linux/Mac 守护进程、服务管理）
- 默认间隔 5 分钟同步一次
- 支持同时配置多个 DNS 服务商与多个域名
- 支持多级域名
- 网页中配置，简单又方便，默认勾选 `禁止从公网访问`
- 网页中方便快速查看最近 50 条日志
- 支持 Webhook 通知
- 支持 TTL
- 支持部分 DNS 服务商传递自定义参数，实现地域解析/多 IP 等功能
- **低资源占用**：release 二进制约 7.7MB，运行内存约 2-8MB
- **OpenWrt 支持**：提供 luci-app 插件（含 ipk/apk 包）

> [!NOTE]
> 建议在启用公网访问时，使用 Nginx 等反向代理软件启用 HTTPS 访问，以保证安全性。

## 系统中使用

- 从 [Releases](https://github.com/jeessy2/ddns-rs/releases) 下载并解压 ddns-rs
- 直接运行：`./ddns-rs`
- 安装服务（可选）
  - Mac/Linux: `sudo ./ddns-rs -s install`
  - Windows(以管理员打开 cmd): `.\ddns-rs.exe -s install`
- 配置
  - 打开浏览器并访问 `http://localhost:9876` 进行初始化配置
- [可选] 服务卸载
  - Mac/Linux: `sudo ./ddns-rs -s uninstall`
  - Windows(以管理员打开 cmd): `.\ddns-rs.exe -s uninstall`

## Docker 中使用

```bash
docker run -d --name ddns-rs --restart=always \
  -p 9876:9876 \
  -v /opt/ddns-rs:/root \
  jeessy/ddns-rs
```

- 浏览器访问 `http://localhost:9876`
- 配置文件保存在 `/opt/ddns-rs` 目录下的 `.ddns_go_config.yaml`（与 DDNS-GO 兼容）

## OpenWrt 中使用

DDNS-RS 提供 LuCI 插件，支持 OpenWrt 24.10（ipk）与 25.x（apk）。

- 从 [Releases](https://github.com/jeessy2/ddns-rs/releases) 下载 `openwrt-*.tar.gz`
- 安装 `luci-app-ddns-rs` 包：`opkg install luci-app-ddns-rs_*.ipk` / `apk add luci-app-ddns-rs_*.apk`
- 安装后，在 LuCI 的 **Services → DDNS-RS** 中配置

> DDNS-RS 的 LuCI 包**不含二进制**。安装后请在 LuCI 的 **Binary** 页面通过上传、自定义链接或一键自动更新安装 ddns-rs 二进制，这样更新 ddns-rs 无需重装 LuCI 包。

## CLI 参数

```
-v                显示版本
-u                自更新到最新版本
-l :9876          监听地址（默认 :9876）
-f 300            更新频率（秒，默认 300）
-c <path>         指定配置文件路径
-s install|uninstall|restart   服务管理
-noweb            不启动 Web 服务
-skipVerify       跳过证书验证
-dns 8.8.8.8      自定义 DNS 服务器
-resetPassword <pwd>  重置密码
-d                后台守护进程运行
-cacheTimes 5     IP 缓存比较次数（默认 5）
```

## 自更新

`./ddns-rs -u` 会从 GitHub Releases 检测并下载最新版本替换当前二进制。

- 默认检测仓库：`jeessy2/ddns-rs`
- 可通过环境变量 `DDNS_RS_REPO` 覆盖为其他仓库（如私有/镜像仓库）

## 界面

- 登录页（首次访问需初始化用户名/密码）
- 配置页（多 DNS 配置、IPv4/IPv6 获取方式、Webhook、TTL）
- 日志页（最近 50 条）

## 开发与自行编译

```bash
# 全量测试
cargo test --workspace

# Release 构建
cargo build --release --bin ddns-rs

# 二进制输出
./target/release/ddns-rs
```

### 项目结构

```
crates/
├── ddns_rs_core/        核心引擎（配置、域名解析、IP 获取、签名、HTTP 客户端）
├── ddns_rs_providers/   全部 DNS 服务商实现
├── ddns_rs_web/         axum Web 界面与 API
└── ddns_rs_cli/         入口、服务管理、自更新
luci/                    OpenWrt 打包（ddns-rs 脚本包 + luci-app-ddns-rs 前端）
.github/workflows/       CI（test / release / openwrt）
```

## 参考项目

- [jeessy2/ddns-go](https://github.com/jeessy2/ddns-go) — 原版 Go 实现，本项目的功能与配置格式参考来源
- [sirpdboy/luci-app-ddns-go](https://github.com/sirpdboy/luci-app-ddns-go) — OpenWrt LuCI 插件，本项目的 luci-app 移植参考

## License

[MIT](LICENSE)
