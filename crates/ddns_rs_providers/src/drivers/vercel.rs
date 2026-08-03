use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

pub struct Vercel {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct ListResp {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    records: Vec<Record>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    id: String,
    name: String,
    value: String,
}

impl Vercel {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let mut ttl = dns_conf.TTL.parse::<i32>().unwrap_or(60);
        if ttl < 60 {
            ttl = 60;
        }
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            ttl,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    fn auth(&self) -> String {
        format!("Bearer {}", self.dns_conf.DNS.Secret)
    }

    fn api_url(&self, api: &str) -> String {
        if self.dns_conf.DNS.ExtParam.is_empty() {
            api.to_string()
        } else {
            let sep = if api.contains('?') { "&" } else { "?" };
            format!("{}{}teamId={}", api, sep, self.dns_conf.DNS.ExtParam)
        }
    }

    async fn list_existing_records(&self, domain: &Domain) -> Result<Vec<Record>, String> {
        let url = self.api_url(&format!(
            "https://api.vercel.com/v4/domains/{}/records",
            domain.domain_name
        ));
        let resp = self
            .http_client
            .get(url)
            .header("Authorization", self.auth())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status() != 200 {
            return Err(format!("Vercel API returned status code {}", resp.status()));
        }
        let result: ListResp = resp.json().await.map_err(|e| e.to_string())?;
        Ok(result.records)
    }

    async fn create_record(&self, domain: &Domain, record_type: &str, ip_addr: &str) -> Result<(), String> {
        let url = self.api_url(&format!(
            "https://api.vercel.com/v2/domains/{}/records",
            domain.domain_name
        ));
        let body = serde_json::json!({
            "name": domain.sub_domain(),
            "type": record_type,
            "value": ip_addr,
            "ttl": self.ttl,
            "comment": "Created by ddns-go"
        });
        let resp = self
            .http_client
            .post(url)
            .header("Authorization", self.auth())
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status() != 200 {
            return Err(format!("Vercel API returned status code {}", resp.status()));
        }
        Ok(())
    }

    async fn update_record(&self, record: &Record, record_type: &str, ip_addr: &str) -> Result<(), String> {
        let url = self.api_url(&format!(
            "https://api.vercel.com/v1/domains/records/{}",
            record.id
        ));
        let body = serde_json::json!({
            "type": record_type,
            "value": ip_addr,
            "ttl": self.ttl
        });
        let resp = self
            .http_client
            .patch(url)
            .header("Authorization", self.auth())
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if resp.status() != 200 {
            return Err(format!("Vercel API returned status code {}", resp.status()));
        }
        Ok(())
    }

    async fn add_update(&mut self, record_type: &str) {
        let (mut ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }
        ip_addr = ip_addr.to_lowercase();

        for mut domain in domains {
            let records = match self.list_existing_records(&domain).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    continue;
                }
            };

            let target_record = records.iter().find(|r| r.name == domain.sub_domain).cloned();

            let operation;
            let result = if let Some(record) = target_record {
                if record.value.to_lowercase() == ip_addr {
                    ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                    domain.update_status = UpdateStatus::Nothing;
                    continue;
                }
                operation = "更新";
                self.update_record(&record, record_type, &ip_addr).await
            } else {
                operation = "新增";
                self.create_record(&domain, record_type, &ip_addr).await
            };

            match result {
                Ok(_) => {
                    ddns_rs_core::log_msg!(
                        "{}域名解析 %s 成功! IP: %s",
                        operation,
                        domain.display(),
                        ip_addr
                    );
                    domain.update_status = UpdateStatus::Success;
                }
                Err(e) => {
                    ddns_rs_core::log_msg!(
                        "{}域名解析 %s 失败! 异常信息: %s",
                        operation,
                        domain.display(),
                        e
                    );
                    domain.update_status = UpdateStatus::Failed;
                }
            }
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Vercel {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
