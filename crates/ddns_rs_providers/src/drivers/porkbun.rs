use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const ENDPOINT: &str = "https://api.porkbun.com/api/json/v3/dns";

pub struct Porkbun {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct ApiKey {
    apikey: String,
    secretapikey: String,
}

#[derive(serde::Serialize, Default)]
struct Record {
    name: Option<String>,
    #[serde(rename = "type")]
    record_type: Option<String>,
    content: Option<String>,
    ttl: Option<String>,
}

#[derive(serde::Deserialize)]
struct QueryResp {
    status: String,
    records: Option<Vec<QueryRecord>>,
}

#[derive(serde::Deserialize)]
struct QueryRecord {
    content: Option<String>,
}

#[derive(serde::Deserialize)]
struct Resp {
    status: String,
}

impl Porkbun {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
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
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    fn api_key(&self) -> ApiKey {
        ApiKey {
            apikey: self.dns_conf.DNS.ID.clone(),
            secretapikey: self.dns_conf.DNS.Secret.clone(),
        }
    }

    async fn request<T: serde::de::DeserializeOwned, D: serde::Serialize + ?Sized>(
        &self,
        url: &str,
        data: Option<&D>,
    ) -> Result<T, String> {
        let mut builder = self.http_client.post(url);
        builder = builder.header("Content-Type", "application/json");
        if let Some(d) = data {
            builder = builder.body(serde_json::to_string(d).unwrap());
        } else {
            builder = builder.body(String::new());
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let query_url = format!(
                "{}/retrieveByNameType/{}/{}/{}",
                ENDPOINT,
                domain.domain_name,
                record_type,
                domain.sub_domain()
            );
            let record: QueryResp = match self.request(&query_url, Some(&self.api_key())).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };
            if record.status == "SUCCESS" {
                let records = record.records.unwrap_or_default();
                if !records.is_empty() {
                    self.modify(&domain, record_type, &ip_addr, records).await;
                } else {
                    self.create(&mut domain, record_type, &ip_addr).await;
                }
            } else {
                ddns_rs_core::log_msg!("在DNS服务商中未找到根域名: %s", domain.domain_name);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let url = format!("{}/create/{}", ENDPOINT, domain.domain_name);
        let body = serde_json::json!({
            "apikey": self.dns_conf.DNS.ID,
            "secretapikey": self.dns_conf.DNS.Secret,
            "name": domain.sub_domain(),
            "type": record_type,
            "content": ip_addr,
            "ttl": self.ttl,
        });
        match self.request::<Resp, serde_json::Value>(&url, Some(&body)).await {
            Ok(response) => {
                if response.status == "SUCCESS" {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), response.status);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn modify(&self, domain: &Domain, record_type: &str, ip_addr: &str, records: Vec<QueryRecord>) {
        if let Some(content) = &records[0].content {
            if *content == ip_addr {
                ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                return;
            }
        }

        let url = format!(
            "{}/editByNameType/{}/{}/{}",
            ENDPOINT,
            domain.domain_name,
            record_type,
            domain.sub_domain()
        );
        let body = serde_json::json!({
            "apikey": self.dns_conf.DNS.ID,
            "secretapikey": self.dns_conf.DNS.Secret,
            "content": ip_addr,
            "ttl": self.ttl,
        });
        let mut domain = domain.clone();
        match self.request::<Resp, serde_json::Value>(&url, Some(&body)).await {
            Ok(response) => {
                if response.status == "SUCCESS" {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), response.status);
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
impl crate::engine::DnsProvider for Porkbun {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
