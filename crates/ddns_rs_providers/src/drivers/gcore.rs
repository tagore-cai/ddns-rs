use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const API_ENDPOINT: &str = "https://api.gcore.com/dns/v2";

pub struct Gcore {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct ZoneResponse {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    zones: Vec<Zone>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Zone {
    id: i32,
    name: String,
}

#[derive(serde::Deserialize, Debug)]
struct RRSetListResponse {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    rrsets: Vec<RRSet>,
}

#[derive(serde::Deserialize, Debug)]
struct RRSet {
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    ttl: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    resource_records: Vec<ResourceRecord>,
}

#[derive(serde::Deserialize, Debug)]
struct ResourceRecord {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    content: Vec<serde_json::Value>,
    enabled: bool,
}

impl Gcore {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = dns_conf.TTL.parse::<i32>().unwrap_or(120);
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            ttl,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        url: &str,
        data: Option<&serde_json::Value>,
    ) -> Result<T, String> {
        let mut builder = self.http_client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            url,
        );
        builder = builder.header("Authorization", format!("APIKey {}", self.dns_conf.DNS.Secret));
        builder = builder.header("Content-Type", "application/json");
        if let Some(d) = data {
            builder = builder.body(d.to_string());
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            ddns_rs_core::log_msg!("返回内容: %s ,返回状态码: %d", text, status.as_u16());
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    async fn get_zone_by_domain(&self, domain: &Domain) -> Result<Option<Zone>, String> {
        let result: ZoneResponse = self
            .request("GET", &format!("{}/zones?name={}", API_ENDPOINT, domain.domain_name), None)
            .await?;
        Ok(result.zones.into_iter().next())
    }

    async fn get_rrset(&self, zone_name: &str, record_name: &str, record_type: &str) -> Result<Option<RRSet>, String> {
        let result: RRSetListResponse = self
            .request("GET", &format!("{}/zones/{}/rrsets", API_ENDPOINT, zone_name), None)
            .await?;
        let full_name = if !record_name.is_empty() && record_name != "@" {
            format!("{}.{}", record_name, zone_name)
        } else {
            zone_name.to_string()
        };
        for rrset in result.rrsets {
            if rrset.name == full_name && rrset.record_type == record_type {
                return Ok(Some(rrset));
            }
        }
        Ok(None)
    }

    fn full_record_name(zone_name: &str, record_name: &str) -> String {
        if record_name.is_empty() || record_name == "@" {
            zone_name.to_string()
        } else {
            format!("{}.{}", record_name, zone_name)
        }
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let zone_info = match self.get_zone_by_domain(&domain).await {
                Ok(z) => z,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };
            let zone = match zone_info {
                Some(z) => z,
                None => {
                    ddns_rs_core::log_msg!("在DNS服务商中未找到根域名: %s", domain.domain_name);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };

            let existing = match self.get_rrset(&zone.name, &domain.sub_domain(), record_type).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };

            if let Some(rrset) = existing {
                self.update_record(&zone.name, &mut domain, record_type, &ip_addr, &rrset).await;
            } else {
                self.create_record(&zone.name, &mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn create_record(&self, zone_name: &str, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let record_name = Self::full_record_name(zone_name, &domain.sub_domain());
        let body = serde_json::json!({
            "ttl": self.ttl,
            "resource_records": [{
                "content": [ip_addr],
                "enabled": true,
            }],
        });
        let url = format!("{}/zones/{}/{}/{}", API_ENDPOINT, zone_name, record_name, record_type);
        let result: Result<serde_json::Value, String> = self.request("POST", &url, Some(&body)).await;
        match result {
            Ok(_) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn update_record(&self, zone_name: &str, domain: &mut Domain, record_type: &str, ip_addr: &str, existing: &RRSet) {
        if let Some(record) = existing.resource_records.first() {
            if let Some(content) = record.content.first() {
                if content.as_str() == Some(ip_addr) {
                    ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                    return;
                }
            }
        }

        let record_name = Self::full_record_name(zone_name, &domain.sub_domain());
        let body = serde_json::json!({
            "ttl": self.ttl,
            "resource_records": [{
                "content": [ip_addr],
                "enabled": true,
            }],
        });
        let url = format!("{}/zones/{}/{}/{}", API_ENDPOINT, zone_name, record_name, record_type);
        let result: Result<serde_json::Value, String> = self.request("PUT", &url, Some(&body)).await;
        match result {
            Ok(_) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Gcore {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
