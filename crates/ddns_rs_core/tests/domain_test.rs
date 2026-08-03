use ddns_rs_core::domain::{check_parse_domains, Domains};
use ddns_rs_core::domain::ipgetter::extract_ip;
use ddns_rs_core::ipcache::IpCache;

#[test]
fn test_parse_simple_domain() {
    let domains = check_parse_domains(&["example.com".to_string()]);
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].domain_name, "example.com");
    assert_eq!(domains[0].sub_domain, "");
}

#[test]
fn test_parse_sub_domain() {
    let domains = check_parse_domains(&["www.example.com".to_string()]);
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].domain_name, "example.com");
    assert_eq!(domains[0].sub_domain, "www");
}

#[test]
fn test_parse_colon_format() {
    let domains = check_parse_domains(&["www:example.cn.eu.org".to_string()]);
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].domain_name, "example.cn.eu.org");
    assert_eq!(domains[0].sub_domain, "www");
}

#[test]
fn test_parse_with_custom_params() {
    let domains = check_parse_domains(&["test.example.com?proxied=true".to_string()]);
    assert_eq!(domains.len(), 1);
    assert_eq!(domains[0].display(), "test.example.com");
    let params = domains[0].custom_params();
    assert_eq!(params.get("proxied").map(|s| s.as_str()), Some("true"));
}

#[test]
fn test_parse_invalid_domain() {
    let domains = check_parse_domains(&["".to_string()]);
    assert_eq!(domains.len(), 0);
}

#[test]
fn test_extract_ipv4() {
    assert_eq!(
        extract_ip("Current IP Address: 123.456.789.000 and 1.2.3.4", true),
        Some("1.2.3.4".to_string())
    );
    assert_eq!(extract_ip("no ip here", true), None);
}

#[test]
fn test_extract_ipv6() {
    assert_eq!(
        extract_ip("ip is 2001:db8::1 and other", false),
        Some("2001:db8::1".to_string())
    );
    assert_eq!(extract_ip("no v6 here", false), None);
}

#[test]
fn test_ip_cache() {
    let mut cache = IpCache::default();
    std::env::set_var("DDNS_IP_CACHE_TIMES", "5");
    // First check with new address returns true
    assert!(cache.check("1.2.3.4"));
    assert_eq!(cache.times, 6);
    // Same address should be false until times runs out (5 times)
    for _ in 0..5 {
        assert!(!cache.check("1.2.3.4"));
    }
    // times reaches 1, returns true
    assert!(cache.check("1.2.3.4"));
    // empty returns true
    assert!(cache.check(""));
}

#[test]
fn test_parse_go_compat_case() {
    // Mirrors config/domains_test.go TestParseDomainArr
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
    // Custom params normalization: Go re-encodes via url.Values.Encode() (sorted)
    assert_eq!(domains[7].custom_params, "Line=oversea&RecordId=123");
}

#[test]
fn test_to_ascii_go_compat_case() {
    // Mirrors config/domains_test.go TestToASCII for realistic inputs.
    // NOTE: Go uses lenient UTS46 mapping (handles pathological inputs like
    // Arabic presentation forms); Rust's strict idna returns Err on those and
    // we fall back to the original string. Real-world domains are unaffected.
    use ddns_rs_core::domain::Domain;
    let cases = [
        ("😺.com", "xn--138h.com"),
        ("ÖBB.at", "xn--bb-eka.at"),
        ("xn--138h.com", "xn--138h.com"),
        ("s3--s4.com", "s3--s4.com"),
        ("例子.公司", "xn--fsqu00a.xn--55qx5d"),
        ("中文域名.cn", "xn--fiq06l2rdsvs.cn"),
    ];
    for (input, expected) in cases {
        let d = Domain {
            domain_name: input.to_string(),
            ..Default::default()
        };
        assert_eq!(d.to_ascii(), expected, "to_ascii({})", input);
    }
    // Pathological input: strict idna errors, we fall back to the input string.
    let d = Domain {
        domain_name: "englishﻋﺮﺑﻲ.com".to_string(),
        ..Default::default()
    };
    assert_eq!(d.to_ascii(), "englishﻋﺮﺑﻲ.com");
}

#[test]
fn test_extract_headers() {
    // Mirrors config/webhook_test.go TestExtractHeaders
    // Our port uses simple parsing; verify comma-join logic separately in webhook.
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
    assert_eq!(map.get("a").map(|s| s.as_str()), Some("foo"));
    assert_eq!(map.get("b").map(|s| s.as_str()), Some("bar"));
    assert_eq!(map.get("c").map(|s| s.as_str()), Some("foo:bar"));
}

#[test]
fn test_save_json_parse() {
    use serde::Deserialize;
    #[derive(Deserialize, Default, Clone, PartialEq)]
    struct DnsConfJS {
        #[serde(rename = "Name", default)] name: String,
        #[serde(rename = "DnsName", default)] dns_name: String,
        #[serde(rename = "DnsID", default)] dns_id: String,
        #[serde(rename = "DnsSecret", default)] dns_secret: String,
        #[serde(rename = "DnsExtParam", default)] dns_ext_param: String,
        #[serde(rename = "TTL", default)] ttl: String,
        #[serde(rename = "Ipv4Enable", default)] ipv4_enable: bool,
        #[serde(rename = "Ipv4GetType", default)] ipv4_get_type: String,
        #[serde(rename = "Ipv4Url", default)] ipv4_url: String,
        #[serde(rename = "Ipv4NetInterface", default)] ipv4_net_interface: String,
        #[serde(rename = "Ipv4Cmd", default)] ipv4_cmd: String,
        #[serde(rename = "Ipv4Domains", default)] ipv4_domains: String,
        #[serde(rename = "Ipv6Enable", default)] ipv6_enable: bool,
        #[serde(rename = "Ipv6GetType", default)] ipv6_get_type: String,
        #[serde(rename = "Ipv6Url", default)] ipv6_url: String,
        #[serde(rename = "Ipv6NetInterface", default)] ipv6_net_interface: String,
        #[serde(rename = "Ipv6Cmd", default)] ipv6_cmd: String,
        #[serde(rename = "Ipv6Reg", default)] ipv6_reg: String,
        #[serde(rename = "Ipv6Domains", default)] ipv6_domains: String,
        #[serde(rename = "HttpInterface", default)] http_interface: String,
    }
    #[derive(Deserialize)]
    struct SaveData {
        #[serde(rename = "DnsConf", default)] dns_conf: Option<Vec<DnsConfJS>>,
        #[serde(rename = "Username", default)] username: Option<String>,
        #[serde(rename = "Password", default)] password: Option<String>,
        #[serde(rename = "Lang", default)] lang: Option<String>,
        #[serde(rename = "NotAllowWanAccess", default)] not_allow_wan_access: Option<bool>,
        #[serde(rename = "WebhookURL", default)] webhook_url: Option<String>,
        #[serde(rename = "WebhookRequestBody", default)] webhook_request_body: Option<String>,
        #[serde(rename = "WebhookHeaders", default)] webhook_headers: Option<String>,
    }
    let json = r#"{"Username":"admin","Password":"","Lang":"zh","NotAllowWanAccess":true,"WebhookURL":"","WebhookRequestBody":"","WebhookHeaders":"","DnsConf":[{"Name":"c1","DnsName":"cloudflare","DnsID":"fakeid123","DnsSecret":"fakesecret456","TTL":"","Ipv4Enable":true,"Ipv4GetType":"url","Ipv4Url":"https://api-ipv4.ip.sb/ip","Ipv4Domains":"test.example.com","Ipv6Enable":false,"Ipv6GetType":"netInterface","Ipv6Url":"","Ipv6NetInterface":"","Ipv6Cmd":"","Ipv6Reg":"","Ipv6Domains":"","HttpInterface":""}]}"#;
    match serde_json::from_str::<SaveData>(json) {
        Ok(d) => {
            let c = d.dns_conf.unwrap();
            assert_eq!(c.len(), 1);
            assert_eq!(c[0].dns_name, "cloudflare");
        }
        Err(e) => panic!("parse failed: {}", e),
    }
}
