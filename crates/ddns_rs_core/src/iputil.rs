use regex::Regex;
use std::sync::LazyLock;

/// IPv4 regex matching Go's Ipv4Reg.
pub static IPV4_REG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])")
        .unwrap()
});

/// Find a valid IPv6 in text. Loosened to catch IPv4-mapped IPv6 like ::ffff:192.168.1.102.
pub fn find_ipv6(text: &str) -> Option<String> {
    // Simple char-range scan for IPv6-like candidates.
    for token in split_ipv6_candidates(text) {
        if let Ok(ip) = token.parse::<std::net::IpAddr>() {
            if ip.is_ipv6() {
                return Some(ip.to_string());
            }
        }
    }
    None
}

fn split_ipv6_candidates(text: &str) -> Vec<String> {
    // Gather substrings of allowed chars ([0-9A-Fa-f:.]) that are long enough.
    let mut result = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_ascii_hexdigit() || c == ':' || c == '.' {
            current.push(c);
        } else {
            if current.len() >= 2 {
                result.push(current.clone());
            }
            current.clear();
        }
    }
    if current.len() >= 2 {
        result.push(current);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_ipv6_plain() {
        assert_eq!(find_ipv6("240e:390:c38a:9b00::1"), Some("240e:390:c38a:9b00::1".to_string()));
    }

    #[test]
    fn test_find_ipv6_in_text() {
        let text = "my address is 240e:390:c38a:9b00::1 and it's great";
        assert_eq!(find_ipv6(text), Some("240e:390:c38a:9b00::1".to_string()));
    }

    #[test]
    fn test_find_ipv4_mapped_ipv6() {
        // IPv4-mapped IPv6 like ::ffff:192.168.1.102 must be found as IPv6.
        assert_eq!(find_ipv6("::ffff:192.168.1.102"), Some("::ffff:192.168.1.102".to_string()));
    }

    #[test]
    fn test_find_ipv6_none_for_ipv4() {
        assert_eq!(find_ipv6("192.168.1.102"), None);
    }

    #[test]
    fn test_find_ipv6_none_for_empty() {
        assert_eq!(find_ipv6(""), None);
    }

    #[test]
    fn test_find_ipv6_none_for_garbage() {
        assert_eq!(find_ipv6("no ip here at all"), None);
        // Bare :: is a valid IPv6 unspecified address.
        assert_eq!(find_ipv6("::"), Some("::".to_string()));
    }
}
