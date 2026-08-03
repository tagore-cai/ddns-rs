use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const DYNADOT_ENDPOINT: &str = "https://www.dynadot.com/set_ddns";

pub struct Dynadot {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    last_ipv4: String,
    last_ipv6: String,
    http_client: reqwest::Client,
}

struct DynadotRecord {
    domain_name: String,
    sub_domain_names: Vec<String>,
    custom_params: BTreeMap<String, String>,
    domains: Vec<Domain>,
    contain_root: bool,
}

#[derive(serde::Deserialize, Debug, Default)]
struct DynadotResp {
    #[serde(default)]
    status: String,
    #[serde(default)]
    error_code: i32,
    #[serde(default)]
    content: Option<Vec<String>>,
}

impl Dynadot {
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
        }
    }

    async fn add_or_update_domain_records(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        // 防止多次发送Webhook通知
        if record_type == "A" {
            if self.last_ipv4 == ip_addr {
                ddns_rs_core::log_msg!("你的IPv4未变化, 未触发 %s 请求", "dynadot");
                return;
            }
        } else if self.last_ipv6 == ip_addr {
            ddns_rs_core::log_msg!("你的IPv6未变化, 未触发 %s 请求", "dynadot");
            return;
        }

        let records = Self::merge_domains(&domains);
        // dynadot 仅支持一个域名对应一个dynamic password
        if records.len() != 1 {
            ddns_rs_core::log_msg!("dynadot仅支持单域名配置，多个域名请添加更多配置");
            return;
        }
        for record in records {
            // 创建或更新
            self.create_or_modify(record, record_type, &ip_addr).await;
        }
    }

    // 合并域名的子域名
    fn merge_domains(domains: &[Domain]) -> Vec<DynadotRecord> {
        let mut records: Vec<DynadotRecord> = Vec::new();
        for domain in domains {
            let mut found = false;
            for record in records.iter_mut() {
                if record.domain_name == domain.domain_name {
                    let params = domain.custom_params();
                    for (k, v) in params {
                        record.custom_params.insert(k, v);
                    }
                    record.domains.push(domain.clone());
                    record.sub_domain_names.push(domain.sub_domain());
                    if domain.sub_domain.is_empty() {
                        // 包含根域名
                        record.contain_root = true;
                    }
                    found = true;
                    break;
                }
            }
            if !found {
                let mut record = DynadotRecord {
                    domain_name: domain.domain_name.clone(),
                    custom_params: domain.custom_params(),
                    domains: vec![domain.clone()],
                    sub_domain_names: vec![domain.sub_domain()],
                    contain_root: false,
                };
                if domain.sub_domain.is_empty() {
                    // 包含根域名
                    record.contain_root = true;
                }
                records.push(record);
            }
        }
        records
    }

    // 创建或变更记录
    async fn create_or_modify(&self, mut record: DynadotRecord, record_type: &str, ip_addr: &str) {
        let mut params = record.custom_params.clone();
        params.insert("domain".to_string(), record.domain_name.clone());
        params.insert("subDomain".to_string(), record.sub_domain_names.join(","));
        params.insert("type".to_string(), record_type.to_string());
        params.insert("ip".to_string(), ip_addr.to_string());
        params.insert("pwd".to_string(), self.dns_conf.DNS.Secret.clone());
        params.insert("ttl".to_string(), self.ttl.clone());
        params.insert("containRoot".to_string(), record.contain_root.to_string());

        let mut result = DynadotResp::default();
        let err = self.request(&params, &mut result).await;

        for domain in record.domains.iter_mut() {
            if let Err(e) = &err {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }

            if result.error_code != -1 {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            } else {
                ddns_rs_core::log_msg!(
                    "更新域名解析 %s 失败! 异常信息: %s",
                    domain.display(),
                    result.content.as_deref().unwrap_or(&[]).join(",")
                );
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    // request 统一请求接口
    async fn request(&self, params: &BTreeMap<String, String>, result: &mut DynadotResp) -> Result<(), String> {
        let query = params
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    url::form_urlencoded::byte_serialize(k.as_bytes()).collect::<String>(),
                    url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>()
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("{}?{}", DYNADOT_ENDPOINT, query);

        let resp = self.http_client.get(&url).send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("返回内容: {} ,返回状态码: {}", text, status.as_u16()));
        }
        *result = serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))?;
        Ok(())
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Dynadot {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_or_update_domain_records("A").await;
        self.add_or_update_domain_records("AAAA").await;
        self.domains.clone()
    }
}
