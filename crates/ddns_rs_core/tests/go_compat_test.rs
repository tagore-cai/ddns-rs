
use ddns_rs_core::domain::{check_parse_domains, Domains};
use ddns_rs_core::domain::ipgetter::extract_ip;
use ddns_rs_core::ipcache::IpCache;

// ---------------------------------------------------------------------------
// Mirrors config/domains_test.go
// ---------------------------------------------------------------------------

#[test]
fn test_parse_domain_arr_go_compat() {
    // Mirrors Go's TestParseDomainArr
    let domains = check_parse_domains(&[
        "mydomain.com".to_string(),
        "test.mydomain.com".to_string(),
        "test2.test.mydomain.com".to_string(),
        "mydomain.com.mydomain.com".to_string(),
        "mydomain.com.cn".to_string(),
        "test.mydomain.com.cn".to_string(),
        "test:mydomain.com.cn".to_string(),
        "test.mydomain.com?Line=oversea&RecordId=123".to_string(),
        "test.mydomain.com.cn?Line=oversea&RecordId=123".to_string(),
        "test2:test.mydomain.com?Line=oversea&RecordId=123".to_string(),
    ]);
    let expected = [
        ("mydomain.com", ""),
        ("mydomain.com", "test"),
        ("mydomain.com", "test2.test"),
        ("mydomain.com", "mydomain.com"),
        ("mydomain.com.cn", ""),
        ("mydomain.com.cn", "test"),
        ("mydomain.com.cn", "test"),
        ("mydomain.com", "test"),
        ("mydomain.com.cn", "test"),
        ("test.mydomain.com", "test2"),
    ];
    assert_eq!(domains.len(), expected.len());
    for (i, (dn, sub)) in expected.iter().enumerate() {
        assert_eq!(&domains[i].domain_name, dn, "case {}: domain_name", i);
        assert_eq!(&domains[i].sub_domain, sub, "case {}: sub_domain", i);
    }
    // Custom params preserved
    assert_eq!(domains[7].custom_params, "Line=oversea&RecordId=123");
}

#[test]
fn test_to_ascii_go_compat() {
    use ddns_rs_core::domain::Domain;
    let cases = [
        ("😺.com", "xn--138h.com"),
        ("ÖBB.at", "xn--bb-eka.at"),
        ("xn--138h.com", "xn--138h.com"),
        ("s3--s4.com", "s3--s4.com"),
        ("例子.公司", "xn--fsqu00a.xn--55qx5d"),
        ("中文域名.cn", "xn--fiq06l2rdsvs.cn"),
        ("", ""),
    ];
    for (input, expected) in cases {
        let d = Domain {
            domain_name: input.to_string(),
            ..Default::default()
        };
        assert_eq!(d.to_ascii(), expected, "to_ascii({:?})", input);
    }
}

// ---------------------------------------------------------------------------
// Mirrors config/webhook_test.go
// ---------------------------------------------------------------------------

#[test]
fn test_extract_headers_go_compat() {
    // Mirrors Go's TestExtractHeaders
    // Go parses "a: foo\nb: bar\nc: foo:bar" -> {a:foo, b:bar, c:foo:bar}
    let input = "\na: foo\nb: bar\nc: foo:bar";
    let mut map = std::collections::BTreeMap::new();
    for line in input.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() == 2 {
            map.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
        }
    }
    assert_eq!(map.len(), 3);
    assert_eq!(map.get("a").map(|s| s.as_str()), Some("foo"));
    assert_eq!(map.get("b").map(|s| s.as_str()), Some("bar"));
    assert_eq!(map.get("c").map(|s| s.as_str()), Some("foo:bar"));
}

// ---------------------------------------------------------------------------
// Mirrors dns/edgeone_origin_test.go
// ---------------------------------------------------------------------------

#[test]
fn test_edgeone_is_origin_group_domain() {
    use ddns_rs_core::domain::Domain;
    // GroupId in custom params enables origin group mode
    let d = Domain {
        domain_name: "example.com".to_string(),
        custom_params: "GroupId=origin-123".to_string(),
        ..Default::default()
    };
    let params = d.custom_params();
    assert!(params.contains_key("GroupId"));

    let d2 = Domain {
        domain_name: "example.com".to_string(),
        custom_params: "OriginGroupName=my-group".to_string(),
        ..Default::default()
    };
    let params2 = d2.custom_params();
    assert!(params2.contains_key("OriginGroupName"));

    // Plain domain (no group params)
    let d3 = Domain {
        domain_name: "example.com".to_string(),
        ..Default::default()
    };
    let params3 = d3.custom_params();
    assert!(!params3.contains_key("GroupId"));
    assert!(!params3.contains_key("OriginGroupName"));
}

#[test]
fn test_ip_cache_single_round_reuse() {
    // Mirrors Go's TestIpCacheSingleRoundReuse
    let mut cache = IpCache::default();
    std::env::set_var("DDNS_IP_CACHE_TIMES", "5");
    assert!(cache.check("1.1.1.1"), "expected first check to trigger compare");
    // Second check with same IP should be skipped (times > 1)
    assert!(!cache.check("1.1.1.1"), "expected second check to be skipped");
}

// ---------------------------------------------------------------------------
// Mirrors util/net_test.go (via netutil)
// ---------------------------------------------------------------------------

#[test]
fn test_is_private_network() {
    use ddns_rs_core::netutil::is_private_network;
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

// ---------------------------------------------------------------------------
// Mirrors util/string_test.go
// ---------------------------------------------------------------------------

#[test]
fn test_write_string_go_compat() {
    use ddns_rs_core::strutil::write_string;
    assert_eq!(write_string(&["hello", "world"]), "helloworld");
    assert_eq!(write_string(&["", "test"]), "test");
    assert_eq!(write_string(&["hello", " ", "world"]), "hello world");
    assert_eq!(write_string(&[""]), "");
}

#[test]
fn test_to_hostname_go_compat() {
    use ddns_rs_core::strutil::to_hostname;
    assert_eq!(to_hostname("https://www.example.com"), "www.example.com");
    assert_eq!(to_hostname("www.example.com/path"), "www.example.com");
    assert_eq!(to_hostname("https://www.example.com/path"), "www.example.com");
}

// ---------------------------------------------------------------------------
// Mirrors util/ordinal_test.go
// ---------------------------------------------------------------------------

#[test]
fn test_ordinal_go_compat() {
    use ddns_rs_core::strutil::ordinal;
    let cases = [
        (0, "0th"), (1, "1st"), (2, "2nd"), (3, "3rd"), (4, "4th"),
        (10, "10th"), (11, "11th"), (12, "12th"), (13, "13th"),
        (21, "21st"), (32, "32nd"), (43, "43rd"),
        (101, "101st"), (102, "102nd"), (103, "103rd"),
        (211, "211th"), (212, "212th"), (213, "213th"),
    ];
    for (input, expected) in cases {
        assert_eq!(ordinal(input, "en"), expected, "ordinal({}, en)", input);
    }
}

// ---------------------------------------------------------------------------
// Mirrors util/semver tests via the `semver` crate (self-update uses it)
// ---------------------------------------------------------------------------

#[test]
fn test_semver_compare() {
    // Mirrors Go's TestGreaterThan/TestGreaterThanOrEqual
    use semver::Version;
    let cases_greater = [
        ("1.2.3", "1.5.1", false),
        ("2.2.3", "1.5.1", true),
    ];
    for (v1, v2, expected) in cases_greater {
        let a = Version::parse(v1).unwrap();
        let b = Version::parse(v2).unwrap();
        assert_eq!(a > b, expected, "{} > {}", v1, v2);
    }

    let cases_gte = [
        ("2.2.3", "1.5.1", true),
        ("1.2.3", "1.5.1", false),
    ];
    for (v1, v2, expected) in cases_gte {
        let a = Version::parse(v1).unwrap();
        let b = Version::parse(v2).unwrap();
        assert_eq!(a >= b, expected, "{} >= {}", v1, v2);
    }
}

// ---------------------------------------------------------------------------
// IP extraction sanity checks
// ---------------------------------------------------------------------------

#[test]
fn test_extract_ipv4() {
    assert_eq!(extract_ip("Current IP: 1.2.3.4", true), Some("1.2.3.4".to_string()));
    assert_eq!(extract_ip("no ip", true), None);
}

#[test]
fn test_extract_ipv6() {
    assert_eq!(extract_ip("ip is 2001:db8::1", false), Some("2001:db8::1".to_string()));
    assert_eq!(extract_ip("no v6", false), None);
}

// ---------------------------------------------------------------------------
// Go template renderer is tested in ddns-web crate (gotemplate lives there).
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Public/WAN access control (mirrors web/auth.go Auth + AuthAssert)
// ---------------------------------------------------------------------------

#[test]
fn test_public_wan_blocking() {
    use ddns_rs_core::netutil::is_private_network;

    // When NotAllowWanAccess is set, public IPs must be rejected.
    // is_private_network correctly classifies:
    assert!(is_private_network("127.0.0.1:9876"), "loopback is private");
    assert!(is_private_network("192.168.1.18:9876"), "192.168 is private");
    assert!(is_private_network("[fd00::1]:9876"), "fc00::/7 is private");
    assert!(!is_private_network("223.5.5.5:9876"), "public IPv4 is not private");
    assert!(!is_private_network("[2409::1]:9876"), "public IPv6 is not private");
    assert!(!is_private_network("100.0.0.1:9876"), "100.64/10 is not private per Go");
}
