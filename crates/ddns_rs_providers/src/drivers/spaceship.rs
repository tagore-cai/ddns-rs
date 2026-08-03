use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const SPACESHIP_API: &str = "https://spaceship.dev/api/v1/dns/records";
const MAX_RECORDS: i32 = 500;

pub struct Spaceship {
    dns_conf: DnsConfig,
    domains: Domains,
    headers: Vec<(String, String)>,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct DataItem {
    #[serde(default)]
    field: String,
    #[serde(default)]
    details: String,
}

#[derive(serde::Deserialize, Debug)]
struct ErrorResponse {
    #[serde(default)]
    detail: String,
    #[serde(default)]
    data: Option<Vec<DataItem>>,
}

#[derive(serde::Deserialize, Debug, Default)]
struct Group {
    #[serde(rename = "type", default)]
    record_type: String,
}

#[derive(serde::Deserialize, Debug, Default)]
struct Item {
    #[serde(rename = "type", default)]
    record_type: String,
    #[serde(default)]
    address: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    ttl: i32,
    #[serde(default)]
    group: Group,
}

#[derive(serde::Deserialize, Debug, Default)]
struct Response {
    #[serde(default)]
    items: Option<Vec<Item>>,
    #[serde(default)]
    total: i32,
}

impl Spaceship {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let mut ttl = 600;
        if let Ok(val) = dns_conf.TTL.parse::<i32>() {
            ttl = val;
        }
        let headers = vec![
            ("X-API-Key".to_string(), dns_conf.DNS.ID.clone()),
            ("X-API-Secret".to_string(), dns_conf.DNS.Secret.clone()),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            headers,
            ttl,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    async fn request(
        &self,
        domain: &Domain,
        method: &str,
        query: &BTreeMap<String, String>,
        payload: &[u8],
    ) -> Result<Vec<u8>, String> {
        let url = format!("{}/{}", SPACESHIP_API, domain.domain_name);
        let mut builder = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), &url);
        for (k, v) in &self.headers {
            builder = builder.header(k, v);
        }
        if !query.is_empty() {
            builder = builder.query(query);
        }
        if !payload.is_empty() {
            builder = builder.body(payload.to_vec());
        }

        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let bytes = resp.bytes().await.map_err(|e| e.to_string())?;

        if status.as_u16() != 200 && status.as_u16() != 204 {
            let e: ErrorResponse = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            return Err(format!("request error: {}", e.detail));
        }
        Ok(bytes.to_vec())
    }

    async fn create_record(&self, record_type: &str, ip: &str, domain: &Domain) -> Result<(), String> {
        let payload = serde_json::json!({
            "force": true,
            "items": [{
                "type": record_type,
                "address": ip,
                "name": domain.sub_domain,
                "ttl": self.ttl,
            }],
        });
        self.request(domain, "PUT", &BTreeMap::new(), &serde_json::to_vec(&payload).unwrap())
            .await?;
        Ok(())
    }

    async fn get_records(&self, record_type: &str, domain: &Domain) -> Result<Vec<String>, String> {
        let mut query = BTreeMap::new();
        query.insert("take".to_string(), MAX_RECORDS.to_string());
        query.insert("skip".to_string(), "0".to_string());

        let resp = self.request(domain, "GET", &query, &[]).await?;
        let response: Response = serde_json::from_slice(&resp).map_err(|e| e.to_string())?;

        if response.total > MAX_RECORDS {
            return Err(format!(
                "could not fetch all {} records in a one request",
                response.total
            ));
        }

        let mut ips = Vec::new();
        for item in response.items.unwrap_or_default() {
            if item.record_type == record_type && item.name == domain.sub_domain {
                ips.push(item.address);
            }
        }
        Ok(ips)
    }

    async fn delete_records(&self, record_type: &str, domain: &Domain, ips: &[String]) -> Result<(), String> {
        if ips.is_empty() {
            return Ok(());
        }
        if ips.len() > MAX_RECORDS as usize {
            return Err(format!(
                "could not delete all {} records in a one request",
                ips.len()
            ));
        }

        let items: Vec<serde_json::Value> = ips
            .iter()
            .map(|ip| {
                serde_json::json!({
                    "type": record_type,
                    "address": ip,
                    "name": domain.sub_domain,
                })
            })
            .collect();
        self.request(domain, "DELETE", &BTreeMap::new(), &serde_json::to_vec(&items).unwrap())
            .await?;
        Ok(())
    }

    async fn update_record(&self, record_type: &str, ip: &str, domain: &Domain) -> Result<bool, String> {
        let ips = self.get_records(record_type, domain).await?;
        if ips.len() == 1 && ips[0] == ip {
            return Ok(false);
        }
        self.delete_records(record_type, domain, &ips).await?;
        self.create_record(record_type, ip, domain).await?;
        Ok(true)
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Spaceship {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        for record_type in ["A", "AAAA"] {
            let (ip, domains) = self.domains.get_new_ip_result(record_type);
            if ip.is_empty() {
                continue;
            }
            for mut domain in domains {
                match self.update_record(record_type, &ip, &domain).await {
                    Ok(has_updated) => {
                        if !has_updated {
                            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip, domain.display());
                        } else {
                            ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip);
                            domain.update_status = UpdateStatus::Success;
                        }
                    }
                    Err(e) => {
                        ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                        domain.update_status = UpdateStatus::Failed;
                    }
                }
            }
        }
        self.domains.clone()
    }
}
