use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const CLOUDNS_ENDPOINT: &str = "https://api.cloudns.net/dns/";

pub struct ClouDNS {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct ClouDNSRecord {
    #[serde(rename = "id", default)]
    id: String,
    #[serde(rename = "type", default)]
    record_type: String,
    #[serde(default)]
    host: String,
    #[serde(rename = "record", default)]
    value: String,
}

#[derive(serde::Deserialize, Debug)]
struct ClouDNSResp {
    #[serde(default)]
    status: String,
    #[serde(rename = "statusDescription", default)]
    status_description: String,
}

impl ClouDNS {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = if dns_conf.TTL.is_empty() {
            // Default 3600 (ClouDNS minimum for some plans)
            "3600".to_string()
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

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            // Get current record information
            let mut params = BTreeMap::new();
            params.insert("auth-id".to_string(), self.dns_conf.DNS.ID.clone());
            params.insert("auth-password".to_string(), self.dns_conf.DNS.Secret.clone());
            params.insert("domain-name".to_string(), domain.domain_name.clone());
            params.insert("host".to_string(), domain.sub_domain());
            params.insert("type".to_string(), record_type.to_string());

            let records: BTreeMap<String, ClouDNSRecord> =
                match self.request("records.json", &params).await {
                    Ok(r) => r,
                    Err(e) => {
                        ddns_rs_core::log_msg!("查询域名 %s 信息发生异常! %v", domain.display(), e);
                        domain.update_status = UpdateStatus::Failed;
                        return;
                    }
                };

            // Find the first record of the matching type and host
            let mut record_selected: Option<ClouDNSRecord> = None;
            if !records.is_empty() {
                for r in records.values() {
                    if r.record_type == record_type && r.host == domain.sub_domain() {
                        record_selected = Some(r.clone());
                        break;
                    }
                }
            }

            if let Some(record) = record_selected {
                // Exist, modify
                self.modify(record, &mut domain, record_type, &ip_addr).await;
            } else {
                // Not exist, create
                self.create(&mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let mut params = BTreeMap::new();
        params.insert("auth-id".to_string(), self.dns_conf.DNS.ID.clone());
        params.insert("auth-password".to_string(), self.dns_conf.DNS.Secret.clone());
        params.insert("domain-name".to_string(), domain.domain_name.clone());
        params.insert("host".to_string(), domain.sub_domain());
        params.insert("type".to_string(), record_type.to_string());
        params.insert("record".to_string(), ip_addr.to_string());
        params.insert("ttl".to_string(), self.ttl.clone());

        let result: Result<ClouDNSResp, String> = self.request("add-record.json", &params).await;
        match result {
            Ok(r) => {
                if r.status == "Success" {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), r.status_description);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %v", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn modify(&self, record_selected: ClouDNSRecord, domain: &mut Domain, _record_type: &str, ip_addr: &str) {
        // Same, no change
        if record_selected.value == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let mut params = BTreeMap::new();
        params.insert("auth-id".to_string(), self.dns_conf.DNS.ID.clone());
        params.insert("auth-password".to_string(), self.dns_conf.DNS.Secret.clone());
        params.insert("domain-name".to_string(), domain.domain_name.clone());
        params.insert("record-id".to_string(), record_selected.id);
        params.insert("host".to_string(), domain.sub_domain());
        params.insert("record".to_string(), ip_addr.to_string());
        params.insert("ttl".to_string(), self.ttl.clone());

        let result: Result<ClouDNSResp, String> = self.request("modify-record.json", &params).await;
        match result {
            Ok(r) => {
                if r.status == "Success" {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), r.status_description);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %v", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        action: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<T, String> {
        let url = format!("{}{}", CLOUDNS_ENDPOINT, action);
        let resp = self.http_client.post(&url).form(params).send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("返回内容: {} ,返回状态码: {}", text, status.as_u16()));
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }
}

#[async_trait]
impl crate::engine::DnsProvider for ClouDNS {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
