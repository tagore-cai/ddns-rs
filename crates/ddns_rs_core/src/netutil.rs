use std::net::IpAddr;

/// Whether an address is a private/loopback/link-local address.
/// Mirrors Go's util.IsPrivateNetwork (excluding the port parsing).
pub fn is_private_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback() // 127/8
                || v4.is_private() // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local() // 169.254/16
        }
        IpAddr::V6(v6) => {
            v6.is_loopback() // ::1
                || is_private_v6(v6) // fc00::/7
                || v6.is_unicast_link_local() // fe80::/10
        }
    }
}

/// IPv6 unique local addresses: fc00::/7
fn is_private_v6(ip: &std::net::Ipv6Addr) -> bool {
    let first = ip.octets()[0];
    (first & 0xfe) == 0xfc
}

/// Check whether a remote address string (optionally with port) is private.
/// Mirrors Go's util.IsPrivateNetwork.
pub fn is_private_network(remote_addr: &str) -> bool {
    let addr = strip_port(remote_addr);
    if let Ok(ip) = addr.parse::<IpAddr>() {
        return is_private_ip(&ip);
    }
    false
}

/// Strip the optional port from a remote address string.
/// - IPv6: "[::1]:9876" -> "::1"
/// - IPv4: "192.168.1.18:9876" -> "192.168.1.18"
fn strip_port(remote_addr: &str) -> &str {
    if let Some(rest) = remote_addr.strip_prefix('[') {
        if let Some(index) = rest.find(']') {
            return &rest[..index];
        }
        return remote_addr;
    }
    if let Some(index) = remote_addr.rfind(':') {
        return &remote_addr[..index];
    }
    remote_addr
}

/// Get IP string from a request-like set of headers.
/// Mirrors Go's util.GetRequestIPStr.
pub fn get_request_ip_str(remote_addr: &str, x_real_ip: Option<&str>, x_forwarded_for: Option<&str>) -> String {
    let mut addr = format!("Remote: {}", remote_addr);
    if let Some(v) = x_real_ip {
        if !v.is_empty() {
            addr = format!("{} ,Real-IP: {}", addr, v);
        }
    }
    if let Some(v) = x_forwarded_for {
        if !v.is_empty() {
            addr = format!("{} ,Forwarded-For: {}", addr, v);
        }
    }
    addr
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_private_network() {
        // Mirrors Go's TestIsPrivateNetwork
        let cases = [
            ("127.0.0.1", true),
            ("127.0.0.1:9876", true),
            ("[::1]", true),
            ("[::1]:9876", true),
            ("192.168.1.18:9876", true),
            ("172.16.1.18:9876", true),
            ("10.1.1.18:9876", true),
            ("[fe80::1]:9876", true),
            ("[fd00::1]:9876", true),
            ("100.0.0.1", false),
            ("100.0.0.1:9876", false),
            ("[2409::1]", false),
            ("[2409::1]:9876", false),
            ("223.5.5.5:9876", false),
        ];
        for (key, value) in cases {
            assert_eq!(is_private_network(key), value, "is_private_network({})", key);
        }
    }

    #[test]
    fn test_get_request_ip_str() {
        // Mirrors Go's TestGetRequestIPStr
        let s = get_request_ip_str(
            "192.168.1.1",
            Some("10.0.0.1"),
            Some("10.0.0.2"),
        );
        assert_eq!(s, "Remote: 192.168.1.1 ,Real-IP: 10.0.0.1 ,Forwarded-For: 10.0.0.2");
    }
}
