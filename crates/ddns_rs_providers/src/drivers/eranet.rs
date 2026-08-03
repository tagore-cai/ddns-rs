use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use std::collections::BTreeMap;

const BASE_URL: &str = "https://www.eranet.com";

pub struct Eranet {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct EranetRecord {
    #[serde(rename = "id")]
    id: i32,
    #[serde(rename = "Domain", default)]
    domain: String,
    #[serde(rename = "Host", default)]
    host: String,
    #[serde(rename = "Type", default)]
    record_type: String,
    #[serde(rename = "Value", default)]
    value: String,
    #[serde(rename = "State", default)]
    state: i32,
}

#[derive(serde::Deserialize, Debug)]
struct EranetRecordListResp {
    #[serde(rename = "RequestId", default)]
    request_id: String,
    #[serde(rename = "Id", default)]
    id: i32,
    #[serde(rename = "error", default)]
    error: String,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    data: Vec<EranetRecord>,
}

#[derive(serde::Deserialize, Debug)]
struct EranetBaseResult {
    #[serde(rename = "RequestId", default)]
    request_id: String,
    #[serde(rename = "Id", default)]
    id: i32,
    #[serde(rename = "error", default)]
    error: String,
}

impl Eranet {
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
            let result = match self.get_record_list(&domain, record_type).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            if !result.data.is_empty() {
                // 默认第一个
                let mut record_selected = result.data[0].clone();
                let params = domain.custom_params();
                if params.contains_key("Id") {
                    for r in &result.data {
                        if r.id.to_string() == params.get("Id").cloned().unwrap_or_default() {
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
        let mut param = BTreeMap::new();
        param.insert("Domain".to_string(), domain.domain_name.clone());
        param.insert("Host".to_string(), domain.sub_domain());
        param.insert("Type".to_string(), record_type.to_string());
        param.insert("Value".to_string(), ip_addr.to_string());
        param.insert("Ttl".to_string(), self.ttl.clone());

        let res = self.request("/api/Dns/AddDomainRecord", &mut param, "GET").await;
        let result: EranetBaseResult = match res {
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            },
        };

        if !result.error.is_empty() {
            ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), result.error);
            domain.update_status = UpdateStatus::Failed;
        } else {
            domain.update_status = UpdateStatus::Success;
        }
    }

    async fn modify(&self, record: EranetRecord, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        // 相同不修改
        if record.value == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let mut param = BTreeMap::new();
        param.insert("Id".to_string(), record.id.to_string());
        param.insert("Domain".to_string(), domain.domain_name.clone());
        param.insert("Host".to_string(), domain.sub_domain());
        param.insert("Type".to_string(), record_type.to_string());
        param.insert("Value".to_string(), ip_addr.to_string());
        param.insert("Ttl".to_string(), self.ttl.clone());

        let res = self.request("/api/Dns/UpdateDomainRecord", &mut param, "GET").await;
        let result: EranetBaseResult = match res {
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
            Ok(bytes) => match serde_json::from_slice(&bytes) {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            },
        };

        if !result.error.is_empty() {
            ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), result.error);
            domain.update_status = UpdateStatus::Failed;
        } else {
            ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
            domain.update_status = UpdateStatus::Success;
        }
    }

    async fn get_record_list(&self, domain: &Domain, typ: &str) -> Result<EranetRecordListResp, String> {
        let mut param = BTreeMap::new();
        param.insert("Domain".to_string(), domain.domain_name.clone());
        param.insert("Type".to_string(), typ.to_string());
        param.insert("Host".to_string(), domain.sub_domain());

        let res = self.request("/api/Dns/DescribeRecordIndex", &mut param, "GET").await?;
        serde_json::from_slice(&res).map_err(|e| e.to_string())
    }

    fn query_params(param: &BTreeMap<String, String>) -> String {
        let mut query_params: Vec<String> = Vec::new();
        for (key, value) in param {
            // 只对键进行URL编码，值保持原样（特别是@符号）
            let encoded_key = url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>();
            let value_str = value;
            // 对值进行选择性编码，保留@符号
            let encoded_value = url::form_urlencoded::byte_serialize(value_str.as_bytes())
                .collect::<String>()
                .replace("%40", "@")
                .replace("%3A", ":");
            query_params.push(format!("{}={}", encoded_key, encoded_value));
        }
        query_params.join("&")
    }

    fn sign(&self, params: &mut BTreeMap<String, String>, method: &str) -> String {
        // 添加公共参数
        params.insert("AccessInstanceID".to_string(), self.dns_conf.DNS.ID.clone());
        params.insert("SignatureMethod".to_string(), "HMAC-SHA1".to_string());
        params.insert(
            "SignatureNonce".to_string(),
            jiff::Timestamp::now().as_nanosecond().to_string(),
        );
        params.insert("Timestamp".to_string(), jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string());

        // 1. 排序参数(按首字母顺序), 排除Signature参数
        let mut keys: Vec<String> = params.keys().cloned().filter(|k| k != "Signature").collect();
        keys.sort();

        // 2. 构造规范化请求字符串
        let canonicalized_query: Vec<String> = keys
            .iter()
            .map(|k| format!("{}={}", percent_encode(k), percent_encode(&params[k])))
            .collect();
        let canonicalized_query_string = canonicalized_query.join("&");

        // 3. 构造待签名字符串
        let string_to_sign = format!("{}&{}&{}", method, percent_encode("/"), percent_encode(&canonicalized_query_string));

        // 4. 计算HMAC-SHA1签名
        let key = format!("{}&", self.dns_conf.DNS.Secret);
        let mut mac = Hmac::<Sha1>::new_from_slice(key.as_bytes()).unwrap();
        mac.update(string_to_sign.as_bytes());
        let signature = B64.encode(mac.finalize().into_bytes());

        // 5. 添加签名到参数中
        params.insert("Signature".to_string(), signature);

        // 6. 重新构造最终的查询字符串(包含签名)
        let mut final_keys: Vec<String> = params.keys().cloned().collect();
        final_keys.sort();
        final_keys
            .iter()
            .map(|k| format!("{}={}", percent_encode(k), percent_encode(&params[k])))
            .collect::<Vec<_>>()
            .join("&")
    }

    async fn request(&self, api_path: &str, params: &mut BTreeMap<String, String>, method: &str) -> Result<Vec<u8>, String> {
        // 生成签名并构造完整URL
        let query_string = self.sign(params, method);
        let full_url = format!("{}{}?{}", BASE_URL, api_path, query_string);

        let resp = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), &full_url)
            .header("Accept", "application/json")
            .send()
            .await
            .map_err(|e| format!("请求失败: {}", e))?;

        let status = resp.status();
        let body = resp.bytes().await.map_err(|e| format!("读取响应失败: {}", e))?;

        if !status.is_success() {
            return Err(format!("API请求失败，状态码: {}, 响应: {}", status.as_u16(), String::from_utf8_lossy(&body)));
        }
        Ok(body.to_vec())
    }
}

fn percent_encode(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    url::form_urlencoded::byte_serialize(s.as_bytes())
        .collect::<String>()
        .replace('+', "%20")
        .replace('*', "%2A")
        .replace("%7E", "~")
}

#[async_trait]
impl crate::engine::DnsProvider for Eranet {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
