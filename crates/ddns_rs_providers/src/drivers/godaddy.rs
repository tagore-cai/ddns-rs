use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

pub struct GoDaddy {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    last_ipv4: String,
    last_ipv6: String,
    http_client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct GoDaddyRecord {
    data: String,
    name: String,
    ttl: i32,
    #[serde(rename = "type")]
    record_type: String,
}

impl GoDaddy {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let last_ipv4 = ipv4_cache.addr.clone();
        let last_ipv6 = ipv6_cache.addr.clone();
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = dns_conf.TTL.parse::<i32>().unwrap_or(600);
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            ttl,
            last_ipv4,
            last_ipv6,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    async fn update_domain_record(&mut self, record_type: &str, ip_addr: &str, domains: &[Domain]) {
        if ip_addr.is_empty() {
            return;
        }

        // Prevent duplicate webhook notifications.
        if record_type == "A" {
            if self.last_ipv4 == ip_addr {
                ddns_rs_core::log_msg!("你的IPv4未变化, 未触发 %s 请求", "godaddy");
                return;
            }
        } else if self.last_ipv6 == ip_addr {
            ddns_rs_core::log_msg!("你的IPv6未变化, 未触发 %s 请求", "godaddy");
            return;
        }

        for mut domain in domains.to_vec() {
            let record = vec![GoDaddyRecord {
                data: ip_addr.to_string(),
                name: domain.sub_domain(),
                ttl: self.ttl,
                record_type: record_type.to_string(),
            }];
            let url = format!(
                "https://api.godaddy.com/v1/domains/{}/records/{}/{}",
                domain.domain_name,
                record_type,
                domain.sub_domain()
            );
            let auth = format!("sso-key {}:{}", self.dns_conf.DNS.ID, self.dns_conf.DNS.Secret);
            match self
                .http_client
                .put(url)
                .header("Authorization", auth)
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&record).unwrap())
                .send()
                .await
            {
                Ok(resp) => {
                    if resp.status().is_success() {
                        ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                        domain.update_status = UpdateStatus::Success;
                    } else {
                        ddns_rs_core::log_msg!(
                            "更新域名解析 %s 失败! 异常信息: %s",
                            domain.display(),
                            resp.status()
                        );
                        domain.update_status = UpdateStatus::Failed;
                    }
                }
                Err(e) => {
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for GoDaddy {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        let (ipv4_addr, ipv4_domains) = self.domains.get_new_ip_result("A");
        if !ipv4_addr.is_empty() {
            self.update_domain_record("A", &ipv4_addr, &ipv4_domains).await;
        }
        let (ipv6_addr, ipv6_domains) = self.domains.get_new_ip_result("AAAA");
        if !ipv6_addr.is_empty() {
            self.update_domain_record("AAAA", &ipv6_addr, &ipv6_domains).await;
        }
        self.domains.clone()
    }
}
