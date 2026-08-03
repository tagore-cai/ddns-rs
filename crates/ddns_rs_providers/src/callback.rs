use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

pub struct Callback {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    last_ipv4: String,
    last_ipv6: String,
    http_client: reqwest::Client,
    ipv4_enable: bool,
    ipv6_enable: bool,
}

impl Callback {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let last_ipv4 = ipv4_cache.addr.clone();
        let last_ipv6 = ipv6_cache.addr.clone();
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = if dns_conf.TTL.is_empty() {
            "600".to_string()
        } else {
            dns_conf.TTL.clone()
        };
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            ttl,
            last_ipv4,
            last_ipv6,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
            ipv4_enable: dns_conf.Ipv4.Enable,
            ipv6_enable: dns_conf.Ipv6.Enable,
        }
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        // Prevent duplicate webhook notifications.
        if record_type == "A" {
            if self.last_ipv4 == ip_addr {
                ddns_rs_core::log_msg!("你的IPv4未变化, 未触发 %s 请求", "Callback");
                return;
            }
        } else if self.last_ipv6 == ip_addr {
            ddns_rs_core::log_msg!("你的IPv6未变化, 未触发 %s 请求", "Callback");
            return;
        }

        for mut domain in domains {
            let method = if self.dns_conf.DNS.Secret.is_empty() {
                "GET"
            } else {
                "POST"
            };
            let post_para = if method == "POST" {
                self.replace_para(&self.dns_conf.DNS.Secret, &ip_addr, &domain, record_type, &self.ttl)
            } else {
                String::new()
            };
            let content_type = if serde_json::from_str::<serde_json::Value>(&post_para).is_ok() {
                "application/json"
            } else {
                "application/x-www-form-urlencoded"
            };

            let request_url = self.replace_para(&self.dns_conf.DNS.ID, &ip_addr, &domain, record_type, &self.ttl);
            let url = match url::Url::parse(&request_url) {
                Ok(u) => u,
                Err(_) => {
                    ddns_rs_core::log_msg!("Callback的URL不正确");
                    return;
                }
            };

            let mut builder = if method == "POST" {
                self.http_client.post(url)
            } else {
                self.http_client.get(url)
            };
            builder = builder.header("content-type", content_type);
            if method == "POST" {
                builder = builder.body(post_para);
            }

            match builder.send().await {
                Ok(resp) => match resp.text().await {
                    Ok(body) => {
                        ddns_rs_core::log_msg!(
                            "Callback调用成功, 域名: %s, IP: %s, 返回数据: %s",
                            domain.display(),
                            ip_addr,
                            body
                        );
                        domain.update_status = UpdateStatus::Success;
                    }
                    Err(e) => {
                        ddns_rs_core::log_msg!("Callback调用失败, 异常信息: %s", e);
                        domain.update_status = UpdateStatus::Failed;
                    }
                },
                Err(e) => {
                    ddns_rs_core::log_msg!("Callback调用失败, 异常信息: %s", e);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
        }
    }

    fn replace_para(&self, org_para: &str, ip_addr: &str, domain: &Domain, record_type: &str, ttl: &str) -> String {
        let mut params = std::collections::HashMap::new();
        params.insert(
            "ip".to_string(),
            ip_addr.to_string(),
        );
        params.insert("domain".to_string(), domain.display());
        params.insert("recordType".to_string(), record_type.to_string());
        params.insert("ttl".to_string(), ttl.to_string());
        params.insert("ipv4Addr".to_string(), self.domains.ipv4_addr.clone());
        params.insert("ipv6Addr".to_string(), self.domains.ipv6_addr.clone());
        params.insert(
            "timestamp".to_string(),
            jiff::Timestamp::now().as_second().to_string(),
        );

        // Also replace custom params of the domain.
        for (k, v) in domain.custom_params() {
            params.insert(k, v);
        }

        let mut result = org_para.to_string();
        for (k, v) in params {
            result = result.replace(&format!("#{{{}}}", k), &v);
        }
        result
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Callback {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        if self.ipv4_enable {
            self.add_update("A").await;
        }
        if self.ipv6_enable {
            self.add_update("AAAA").await;
        }
        self.domains.clone()
    }
}
