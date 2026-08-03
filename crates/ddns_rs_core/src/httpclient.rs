use crate::netiface;
use std::net::IpAddr;
use std::sync::OnceLock;
use std::time::Duration;

static INSECURE_SKIP_VERIFY: OnceLock<bool> = OnceLock::new();

/// Set global TLS skip-verify.
pub fn set_insecure_skip_verify() {
    let _ = INSECURE_SKIP_VERIFY.set(true);
}

fn skip_verify() -> bool {
    *INSECURE_SKIP_VERIFY.get().unwrap_or(&false)
}

/// Create the default HTTP client.
pub fn create_http_client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(30))
        .pool_max_idle_per_host(100)
        .tcp_keepalive(Duration::from_secs(30));
    if skip_verify() {
        builder = builder.danger_accept_invalid_certs(true);
    }
    builder.build().unwrap_or_default()
}

/// Create a no-proxy HTTP client for the given network family (tcp4/tcp6).
pub fn create_no_proxy_client(network: &str) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(30))
        .no_proxy()
        .tcp_keepalive(Duration::from_secs(30));
    if skip_verify() {
        builder = builder.danger_accept_invalid_certs(true);
    }
    if network == "tcp4" {
        builder = builder.local_address(Some(local_ip_v4().unwrap_or(std::net::Ipv4Addr::UNSPECIFIED.into())));
    } else if network == "tcp6" {
        if let Some(ip) = local_ip_v6() {
            builder = builder.local_address(Some(ip));
        }
    }
    builder.build().unwrap_or_default()
}

fn local_ip_v4() -> Option<IpAddr> {
    if let Ok((ipv4, _)) = netiface::get_net_interface() {
        for iface in ipv4 {
            if !iface.address.is_empty() {
                return iface.address[0].parse().ok();
            }
        }
    }
    None
}

fn local_ip_v6() -> Option<IpAddr> {
    if let Ok((_, ipv6)) = netiface::get_net_interface() {
        for iface in ipv6 {
            if !iface.address.is_empty() {
                return iface.address[0].parse().ok();
            }
        }
    }
    None
}

/// Create a bound HTTP client for a specific interface (empty = default).
pub fn bound_client(iface_name: &str, _network: &str) -> reqwest::Client {
    if iface_name.is_empty() {
        return create_no_proxy_client(_network);
    }
    match netiface::get_local_addr_from_interface(iface_name, _network) {
        Ok(ip) => {
            let mut builder = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(30))
                .no_proxy()
                .local_address(Some(ip))
                .tcp_keepalive(Duration::from_secs(30));
            if skip_verify() {
                builder = builder.danger_accept_invalid_certs(true);
            }
            builder.build().unwrap_or_default()
        }
        Err(e) => {
            crate::log_msg!(
                "绑定网卡失败, 将使用默认网卡. 网卡: %s, 错误: %v",
                iface_name,
                e
            );
            create_no_proxy_client(_network)
        }
    }
}

/// Create an HTTP client honoring custom DNS and interface binding.
pub fn create_http_client_with_interface(iface_name: &str) -> reqwest::Client {
    if iface_name.is_empty() {
        return create_http_client();
    }
    match netiface::get_local_addr_from_interface(iface_name, "") {
        Ok(ip) => {
            let mut builder = reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .connect_timeout(Duration::from_secs(30))
                .local_address(Some(ip))
                .tcp_keepalive(Duration::from_secs(30));
            if skip_verify() {
                builder = builder.danger_accept_invalid_certs(true);
            }
            builder.build().unwrap_or_default()
        }
        Err(e) => {
            crate::log_msg!(
                "绑定网卡失败, 将使用默认网卡. 网卡: %s, 错误: %v",
                iface_name,
                e
            );
            create_http_client()
        }
    }
}

/// Shared custom DNS server (set via -dns flag).
static CUSTOM_DNS: OnceLock<String> = OnceLock::new();

/// Set the custom DNS server.
pub fn set_dns(dns: &str) {
    let _ = CUSTOM_DNS.set(dns.to_string());
}

/// Whether a custom DNS server is configured.
pub fn custom_dns() -> Option<&'static str> {
    CUSTOM_DNS.get().map(|s| s.as_str())
}

/// Resolve a host to socket addresses, honoring custom DNS.
pub fn resolve_host(host: &str, port: u16) -> Result<Vec<std::net::SocketAddr>, String> {
    if let Some(dns) = custom_dns() {
        return netiface::resolve_with_dns(host, port, dns);
    }
    use std::net::ToSocketAddrs;
    (host, port)
        .to_socket_addrs()
        .map(|iter| iter.collect())
        .map_err(|e| e.to_string())
}
