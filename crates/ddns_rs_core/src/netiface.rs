use std::net::{IpAddr, SocketAddr, TcpStream};

/// A network interface with its addresses.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct NetInterface {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Address")]
    pub address: Vec<String>,
}

/// Get network interfaces, returns (ipv4, ipv6) lists.
pub fn get_net_interface() -> Result<(Vec<NetInterface>, Vec<NetInterface>), String> {
    let mut ipv4 = Vec::new();
    let mut ipv6 = Vec::new();

    let ifaces = get_if_addrs::get_if_addrs().map_err(|e| e.to_string())?;
    for iface in ifaces {
        if !iface.is_loopback() {
            let name = iface.name.clone();
            let ip = iface.ip();
            if ip.is_ipv4() && is_global(&ip) {
                push_addr(&mut ipv4, &name, &ip.to_string());
            } else if ip.is_ipv6() && is_global_unicast_v6(&ip) {
                push_addr(&mut ipv6, &name, &ip.to_string());
            }
        }
    }

    Ok((ipv4, ipv6))
}

fn push_addr(list: &mut Vec<NetInterface>, name: &str, addr: &str) {
    if let Some(iface) = list.iter_mut().find(|i| i.name == name) {
        if !iface.address.iter().any(|a| a == addr) {
            iface.address.push(addr.to_string());
        }
    } else {
        list.push(NetInterface {
            name: name.to_string(),
            address: vec![addr.to_string()],
        });
    }
}

/// Global unicast IPv6 check: match 2000::/3 and not link-local/unique-local.
fn is_global_unicast_v6(ip: &IpAddr) -> bool {
    if let IpAddr::V6(v6) = ip {
        let first = v6.octets()[0];
        // 2000::/3 => first byte between 0x20 and 0x3f
        return (0x20..=0x3f).contains(&first) && !v6.is_loopback();
    }
    false
}

/// Get a local IP address of the interface matching the network family (tcp4/tcp6).
pub fn get_local_addr_from_interface(iface_name: &str, network: &str) -> Result<IpAddr, String> {
    let ifaces = get_if_addrs::get_if_addrs().map_err(|e| e.to_string())?;
    let mut has_global = false;
    for iface in ifaces {
        if iface.name != iface_name {
            continue;
        }
        let ip = iface.ip();
        if !is_global(&ip) {
            continue;
        }
        has_global = true;
        match network {
            "tcp4" => {
                if ip.is_ipv4() {
                    return Ok(ip);
                }
            }
            "tcp6" => {
                if ip.is_ipv6() {
                    return Ok(ip);
                }
            }
            _ => return Ok(ip),
        }
    }
    if has_global {
        return Err(format!("interface {} has no usable {} address", iface_name, network));
    }
    Err(format!(
        "interface {} has no usable global-unicast address",
        iface_name
    ))
}

fn is_global(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !v4.is_private()
            && !v4.is_loopback()
            && !v4.is_link_local()
            && !v4.is_broadcast(),
        IpAddr::V6(v6) => {
            let first = v6.octets()[0];
            (0x20..=0x3f).contains(&first) && !v6.is_loopback()
        }
    }
}

/// Compatibility helper: resolve a domain with a custom DNS server.
pub fn resolve_with_dns(host: &str, port: u16, dns: &str) -> Result<Vec<SocketAddr>, String> {
    use hickory_resolver::config::{NameServerConfig, Protocol, ResolverConfig, ResolverOpts};
    use hickory_resolver::TokioAsyncResolver;

    let mut config = ResolverConfig::default();
    let addr = dns
        .parse::<SocketAddr>()
        .map_err(|e| format!("invalid DNS server {}: {}", dns, e))?;
    config.add_name_server(NameServerConfig::new(addr, Protocol::Udp));

    let resolver = TokioAsyncResolver::tokio(config, ResolverOpts::default());
    let runtime = tokio::runtime::Handle::current();
    let result = runtime.block_on(async {
        let lookup = resolver.lookup_ip(host).await?;
        let addrs: Vec<SocketAddr> = lookup
            .iter()
            .map(|ip| SocketAddr::new(ip, port))
            .collect();
        Ok::<Vec<SocketAddr>, Box<dyn std::error::Error + Send + Sync>>(addrs)
    });
    result.map_err(|e| e.to_string())
}

/// Wait for the target hosts to be reachable.
pub fn wait_internet(hosts: &[&str]) {
    let timeout = std::time::Duration::from_secs(300);
    let start = std::time::Instant::now();
    loop {
        for host in hosts {
            if let Some((h, p)) = split_host_port(host) {
                if let Ok(ip) = h.parse::<std::net::IpAddr>() {
                    if TcpStream::connect_timeout(&SocketAddr::new(ip, p), std::time::Duration::from_secs(5)).is_ok()
                    {
                        crate::log_msg!("网络已连接");
                        return;
                    }
                }
            }
        }
        if start.elapsed() > timeout {
            return;
        }
        crate::log_msg!("等待网络连接: %s", "network not ready");
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

fn split_host_port(host: &str) -> Option<(String, u16)> {
    if let Ok(sa) = host.parse::<SocketAddr>() {
        return Some((sa.ip().to_string(), sa.port()));
    }
    let (h, p) = host.rsplit_once(':')?;
    Some((h.to_string(), p.parse().ok()?))
}
