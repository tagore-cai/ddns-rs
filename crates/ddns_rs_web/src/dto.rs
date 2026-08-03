use ddns_rs_core::config;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginData {
    #[serde(rename = "Username")]
    pub username: String,
    #[serde(rename = "Password")]
    pub password: String,
}

#[derive(Deserialize)]
pub struct SaveData {
    #[serde(rename = "Username")]
    pub username: Option<String>,
    #[serde(rename = "Password")]
    pub password: Option<String>,
    #[serde(rename = "Lang")]
    pub lang: Option<String>,
    #[serde(rename = "NotAllowWanAccess")]
    pub not_allow_wan_access: Option<bool>,
    #[serde(rename = "WebhookURL")]
    pub webhook_url: Option<String>,
    #[serde(rename = "WebhookRequestBody")]
    pub webhook_request_body: Option<String>,
    #[serde(rename = "WebhookHeaders")]
    pub webhook_headers: Option<String>,
    #[serde(rename = "DnsConf")]
    pub dns_conf: Option<Vec<DnsConfJS>>,
}

#[derive(Deserialize, Serialize, Default, Clone, PartialEq)]
pub struct DnsConfJS {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "DnsName", default)]
    pub dns_name: String,
    #[serde(rename = "DnsID", default)]
    pub dns_id: String,
    #[serde(rename = "DnsSecret", default)]
    pub dns_secret: String,
    #[serde(rename = "DnsExtParam", default)]
    pub dns_ext_param: String,
    #[serde(rename = "TTL", default)]
    pub ttl: String,
    #[serde(rename = "Ipv4Enable", default)]
    pub ipv4_enable: bool,
    #[serde(rename = "Ipv4GetType", default)]
    pub ipv4_get_type: String,
    #[serde(rename = "Ipv4Url", default)]
    pub ipv4_url: String,
    #[serde(rename = "Ipv4NetInterface", default)]
    pub ipv4_net_interface: String,
    #[serde(rename = "Ipv4Cmd", default)]
    pub ipv4_cmd: String,
    #[serde(rename = "Ipv4Domains", default)]
    pub ipv4_domains: String,
    #[serde(rename = "Ipv6Enable", default)]
    pub ipv6_enable: bool,
    #[serde(rename = "Ipv6GetType", default)]
    pub ipv6_get_type: String,
    #[serde(rename = "Ipv6Url", default)]
    pub ipv6_url: String,
    #[serde(rename = "Ipv6NetInterface", default)]
    pub ipv6_net_interface: String,
    #[serde(rename = "Ipv6Cmd", default)]
    pub ipv6_cmd: String,
    #[serde(rename = "Ipv6Reg", default)]
    pub ipv6_reg: String,
    #[serde(rename = "Ipv6Domains", default)]
    pub ipv6_domains: String,
    #[serde(rename = "HttpInterface", default)]
    pub http_interface: String,
}

pub fn hide_id_secret(id: &str, secret: &str, name: &str) -> (String, String) {
    let display_count = 3;
    let hide = |s: &str| -> String {
        if s.len() > display_count && name != "callback" {
            format!("{}{}", &s[..display_count], "*".repeat(s.len() - display_count))
        } else {
            s.to_string()
        }
    };
    (hide(id), hide(secret))
}

pub fn dns_conf_to_js(conf: &config::Config) -> Vec<DnsConfJS> {
    conf.DnsConf
        .iter()
        .map(|c| {
            let (dns_id, dns_secret) = hide_id_secret(&c.DNS.ID, &c.DNS.Secret, &c.DNS.Name);
            DnsConfJS {
                name: c.Name.clone(),
                dns_name: c.DNS.Name.clone(),
                dns_id,
                dns_secret,
                dns_ext_param: c.DNS.ExtParam.clone(),
                ttl: c.TTL.clone(),
                ipv4_enable: c.Ipv4.Enable,
                ipv4_get_type: c.Ipv4.GetType.clone(),
                ipv4_url: c.Ipv4.URL.clone(),
                ipv4_net_interface: c.Ipv4.NetInterface.clone(),
                ipv4_cmd: c.Ipv4.Cmd.clone(),
                ipv4_domains: c.Ipv4.Domains.join("\r\n"),
                ipv6_enable: c.Ipv6.Enable,
                ipv6_get_type: c.Ipv6.GetType.clone(),
                ipv6_url: c.Ipv6.URL.clone(),
                ipv6_net_interface: c.Ipv6.NetInterface.clone(),
                ipv6_cmd: c.Ipv6.Cmd.clone(),
                ipv6_reg: c.Ipv6.Ipv6Reg.clone(),
                ipv6_domains: c.Ipv6.Domains.join("\r\n"),
                http_interface: c.HttpInterface.clone(),
            }
        })
        .collect()
}
