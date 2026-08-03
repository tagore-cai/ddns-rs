use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const RECORD_LIST_API: &str = "https://dnsapi.cn/Record.List";
const RECORD_MODIFY_URL: &str = "https://dnsapi.cn/Record.Modify";
const RECORD_CREATE_API: &str = "https://dnsapi.cn/Record.Create";

pub struct Dnspod {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "value")]
    value: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordListResp {
    #[serde(rename = "status")]
    status: Status,
    #[serde(rename = "records")]
    records: Option<Vec<Record>>,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResp {
    #[serde(rename = "status")]
    status: Status,
}

#[derive(serde::Deserialize, Debug)]
struct Status {
    #[serde(rename = "code")]
    code: String,
    #[serde(rename = "message")]
    message: String,
}

impl Dnspod {
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

    fn base_params(&self, domain: &Domain) -> BTreeMap<String, String> {
        let mut params = domain.custom_params();
        params.insert("login_token".into(), format!("{},{}", self.dns_conf.DNS.ID, self.dns_conf.DNS.Secret));
        params.insert("domain".into(), domain.domain_name.clone());
        params.insert("format".into(), "json".into());
        params
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            match self.get_record_list(&domain, record_type).await {
                Ok(result) => {
                    let records = result.records.unwrap_or_default();
                    if !records.is_empty() {
                        let mut record_selected = records[0].clone();
                        if let Some(rid) = domain.custom_params().get("record_id") {
                            for r in &records {
                                if &r.id == rid {
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
        let mut params = self.base_params(domain);
        params.insert("sub_domain".into(), domain.sub_domain());
        params.insert("record_type".into(), record_type.into());
        params.insert("value".into(), ip_addr.into());
        params.insert("ttl".into(), self.ttl.clone());
        if !params.contains_key("record_line") {
            params.insert("record_line".into(), "默认".into());
        }

        match self.request::<StatusResp>(RECORD_CREATE_API, &params).await {
            Ok(result) => {
                if result.status.code == "1" {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), result.status.message);
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

        let mut params = self.base_params(domain);
        params.insert("sub_domain".into(), domain.sub_domain());
        params.insert("record_type".into(), record_type.into());
        params.insert("value".into(), ip_addr.into());
        params.insert("ttl".into(), self.ttl.clone());
        params.insert("record_id".into(), record.id);
        if !params.contains_key("record_line") {
            params.insert("record_line".into(), "默认".into());
        }

        match self.request::<StatusResp>(RECORD_MODIFY_URL, &params).await {
            Ok(result) => {
                if result.status.code == "1" {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), result.status.message);
                    domain.update_status = UpdateStatus::Failed;
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn get_record_list(&self, domain: &Domain, record_type: &str) -> Result<RecordListResp, String> {
        let mut params = self.base_params(domain);
        params.insert("record_type".into(), record_type.into());
        params.insert("sub_domain".into(), domain.sub_domain());
        self.request(RECORD_LIST_API, &params).await
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        api: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<T, String> {
        let resp = self
            .http_client
            .post(api)
            .form(params)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            ddns_rs_core::log_msg!("返回内容: %s ,返回状态码: %d", text, status.as_u16());
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Dnspod {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
