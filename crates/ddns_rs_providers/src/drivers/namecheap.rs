use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const ENDPOINT: &str =
    "https://dynamicdns.park-your-domain.com/update?host=#{host}&domain=#{domain}&password=#{password}&ip=#{ip}";

pub struct NameCheap {
    dns_conf: DnsConfig,
    domains: Domains,
    last_ipv4: String,
    last_ipv6: String,
    http_client: reqwest::Client,
}

impl NameCheap {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let last_ipv4 = ipv4_cache.addr.clone();
        let last_ipv6 = ipv6_cache.addr.clone();
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            last_ipv4,
            last_ipv6,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        if record_type == "A" {
            if self.last_ipv4 == ip_addr {
                ddns_rs_core::log_msg!("你的IPv4未变化, 未触发 %s 请求", "NameCheap");
                return;
            }
        } else {
            ddns_rs_core::log_msg!("Namecheap 不支持更新 IPv6");
            return;
        }

        for mut domain in domains {
            self.modify(&mut domain, &ip_addr).await;
        }
    }

    async fn modify(&self, domain: &mut Domain, ip_addr: &str) {
        let url = ENDPOINT
            .replace("#{host}", &domain.sub_domain())
            .replace("#{domain}", &domain.domain_name)
            .replace("#{password}", &self.dns_conf.DNS.Secret)
            .replace("#{ip}", ip_addr);

        match self.http_client.get(url).send().await {
            Ok(resp) => {
                let status = resp.text().await.unwrap_or_default();
                if status.contains("<ErrCount>0</ErrCount>") {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), status);
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

#[async_trait]
impl crate::engine::DnsProvider for NameCheap {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
