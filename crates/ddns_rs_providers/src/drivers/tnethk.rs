use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const BASE_URL: &str = "https://www.tnet.hk";

pub struct Tnethk {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    id: i32,
    domain: String,
    host: String,
    #[serde(rename = "type")]
    record_type: String,
    value: String,
    state: i32,
}

#[derive(serde::Deserialize, Debug)]
struct ListResp {
    request_id: String,
    id: i32,
    error: String,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    data: Vec<Record>,
}

#[derive(serde::Deserialize, Debug)]
struct BaseResult {
    request_id: String,
    id: i32,
    error: String,
}

impl Tnethk {
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

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        api_path: &str,
        params: &mut BTreeMap<String, String>,
    ) -> Result<T, String> {
        let query = ddns_rs_core::signer::aliyun_style_query_sign(
            &self.dns_conf.DNS.ID,
            &self.dns_conf.DNS.Secret,
            params,
        );
        let url = format!("{}{}?{}", BASE_URL, api_path, query);
        let resp = self
            .http_client
            .get(url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("API请求失败，状态码: {}, 响应: {}", status.as_u16(), text));
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    async fn get_record_list(&self, domain: &Domain, record_type: &str) -> Result<ListResp, String> {
        let mut params = BTreeMap::new();
        params.insert("Domain".into(), domain.domain_name.clone());
        params.insert("Type".into(), record_type.into());
        params.insert("Host".into(), domain.sub_domain());
        self.request("/api/Dns/DescribeRecordIndex", &mut params).await
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let result = match self.get_record_list(&domain, record_type).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            if !result.data.is_empty() {
                let mut record_selected = result.data[0].clone();
                if let Some(id) = domain.custom_params().get("Id") {
                    for r in &result.data {
                        if r.id.to_string() == *id {
                            record_selected = r.clone();
                        }
                    }
                }
                self.modify(record_selected, &mut domain, record_type, &ip_addr).await;
            } else {
                self.create(&mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let mut params = BTreeMap::new();
        params.insert("Domain".into(), domain.domain_name.clone());
        params.insert("Host".into(), domain.sub_domain());
        params.insert("Type".into(), record_type.into());
        params.insert("Value".into(), ip_addr.into());
        params.insert("Ttl".into(), self.ttl.clone());

        let result: Result<BaseResult, String> = self.request("/api/Dns/AddDomainRecord", &mut params).await;
        match result {
            Ok(r) if r.error.is_empty() => {
                ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            }
            Ok(r) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), r.error);
                domain.update_status = UpdateStatus::Failed;
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
        let mut params = BTreeMap::new();
        params.insert("Id".into(), record.id.to_string());
        params.insert("Domain".into(), domain.domain_name.clone());
        params.insert("Host".into(), domain.sub_domain());
        params.insert("Type".into(), record_type.into());
        params.insert("Value".into(), ip_addr.into());
        params.insert("Ttl".into(), self.ttl.clone());

        let result: Result<BaseResult, String> = self.request("/api/Dns/UpdateDomainRecord", &mut params).await;
        match result {
            Ok(r) if r.error.is_empty() => {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            }
            Ok(r) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), r.error);
                domain.update_status = UpdateStatus::Failed;
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Tnethk {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
