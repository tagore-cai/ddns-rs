use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const ENDPOINT: &str = "https://alidns.aliyuncs.com/";

pub struct Alidns {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    #[serde(rename = "RecordId")]
    record_id: String,
    #[serde(rename = "Value")]
    value: String,
}

#[derive(serde::Deserialize, Debug)]
struct SubDomainRecords {
    #[serde(rename = "TotalCount")]
    total_count: i32,
    #[serde(rename = "DomainRecords")]
    domain_records: DomainRecords,
}

#[derive(serde::Deserialize, Debug)]
struct DomainRecords {
    #[serde(rename = "Record")]
    record: Option<Vec<Record>>,
}

#[derive(serde::Deserialize, Debug)]
struct ApiResp {
    #[serde(rename = "RecordId")]
    record_id: Option<String>,
    #[serde(rename = "Code")]
    code: Option<String>,
    #[serde(rename = "Message")]
    message: Option<String>,
}

impl Alidns {
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

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let mut params = domain.custom_params();
            params.insert("Action".into(), "DescribeSubDomainRecords".into());
            params.insert("DomainName".into(), domain.domain_name.clone());
            params.insert("SubDomain".into(), domain.full_domain());
            params.insert("Type".into(), record_type.into());

            let records: Result<SubDomainRecords, String> = self.request(&mut params).await;
            match records {
                Ok(records) => {
                    if records.total_count > 0 {
                        let records_vec = records.domain_records.record.unwrap_or_default();
                        let mut record_selected = records_vec[0].clone();
                        if let Some(record_id) = params.get("RecordId") {
                            for r in &records_vec {
                                if &r.record_id == record_id {
                                    record_selected = r.clone();
                                }
                            }
                        }
                        self.modify(record_selected, &mut domain, record_type, &ip_addr).await;
                    } else {
                        self.create(&mut domain, record_type, &ip_addr).await;
                    }
                }
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let mut params = domain.custom_params();
        params.insert("Action".into(), "AddDomainRecord".into());
        params.insert("DomainName".into(), domain.domain_name.clone());
        params.insert("RR".into(), domain.sub_domain());
        params.insert("Type".into(), record_type.into());
        params.insert("Value".into(), ip_addr.into());
        params.insert("TTL".into(), self.ttl.clone());

        match self.request::<ApiResp>(&mut params).await {
            Ok(result) => {
                if result.record_id.is_some() {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!(
                        "新增域名解析 %s 失败! 异常信息: %s",
                        domain.display(),
                        result.message.unwrap_or_default()
                    );
                    domain.update_status = UpdateStatus::Failed;
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn modify(&self, record: Record, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        if record.value == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let mut params = domain.custom_params();
        params.insert("Action".into(), "UpdateDomainRecord".into());
        params.insert("RR".into(), domain.sub_domain());
        params.insert("RecordId".into(), record.record_id);
        params.insert("Type".into(), record_type.into());
        params.insert("Value".into(), ip_addr.into());
        params.insert("TTL".into(), self.ttl.clone());

        match self.request::<ApiResp>(&mut params).await {
            Ok(result) => {
                if result.record_id.is_some() {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!(
                        "更新域名解析 %s 失败! 异常信息: %s",
                        domain.display(),
                        result.message.unwrap_or_default()
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

    async fn request<T: serde::de::DeserializeOwned>(&self, params: &mut BTreeMap<String, String>) -> Result<T, String> {
        ddns_rs_core::signer::aliyun_sign(
            &self.dns_conf.DNS.ID,
            &self.dns_conf.DNS.Secret,
            params,
            "GET",
            "2015-01-09",
        );
        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("{}?{}", ENDPOINT, query);

        let resp = self.http_client.get(url).send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            ddns_rs_core::log_msg!("返回内容: %s ,返回状态码: %d", text, status.as_u16());
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Alidns {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
