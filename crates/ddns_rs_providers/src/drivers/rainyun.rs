use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;

const RAINYUN_ENDPOINT: &str = "https://api.v2.rainyun.com";

pub struct Rainyun {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct RainyunRecord {
    #[serde(rename = "record_id", default)]
    record_id: i64,
    #[serde(rename = "host", default)]
    host: String,
    #[serde(rename = "type", default)]
    record_type: String,
    #[serde(rename = "value", default)]
    value: String,
    #[serde(rename = "line", default)]
    line: String,
    #[serde(rename = "ttl", default)]
    ttl: i32,
    #[serde(rename = "level", default)]
    level: i32,
}

#[derive(serde::Deserialize, Debug)]
struct RainyunResp {
    #[serde(default)]
    code: i32,
    #[serde(default)]
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

#[derive(serde::Deserialize, Debug, Default)]
struct RecordListData {
    #[serde(rename = "TotalRecords", default)]
    total_records: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    records: Vec<RainyunRecord>,
}

impl Rainyun {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = if dns_conf.TTL.is_empty() {
            600
        } else {
            dns_conf.TTL.parse::<i32>().unwrap_or(0)
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
            // 获取Domain ID
            let domain_id = self.dns_conf.DNS.ID.clone();

            // 获取记录列表
            let records = match self.get_record_list(&domain_id).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };

            // 查找匹配的记录
            let mut record_selected: Option<RainyunRecord> = None;
            for r in &records {
                if r.host.eq_ignore_ascii_case(&domain.sub_domain())
                    && r.record_type.eq_ignore_ascii_case(record_type)
                {
                    record_selected = Some(r.clone());
                    break;
                }
            }

            if let Some(record) = record_selected {
                // 更新记录
                self.modify(&domain_id, &record, &mut domain, &ip_addr).await;
            } else {
                // 新增记录
                self.create(&domain_id, &mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn get_record_list(&self, domain_id: &str) -> Result<Vec<RainyunRecord>, String> {
        let mut query = BTreeMap::new();
        query.insert("limit".to_string(), "100".to_string());
        query.insert("page_no".to_string(), "1".to_string());

        let path = format!("/product/domain/{}/dns/", path_escape(domain_id));
        let mut result = RecordListData::default();
        self.request::<RecordListData>("GET", &path, Some(&query), None, Some(&mut result))
            .await?;
        Ok(result.records)
    }

    async fn create(&self, domain_id: &str, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let record = RainyunRecord {
            record_id: 0,
            host: domain.sub_domain(),
            record_type: record_type.to_string(),
            value: ip_addr.to_string(),
            line: "DEFAULT".to_string(),
            ttl: self.ttl,
            level: 10,
        };

        match self.create_record(domain_id, &record).await {
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

    async fn create_record(&self, domain_id: &str, record: &RainyunRecord) -> Result<(), String> {
        let payload = serde_json::json!({
            "host": record.host,
            "line": record.line,
            "level": record.level,
            "ttl": record.ttl,
            "type": record.record_type,
            "value": record.value,
            "record_id": 0,
        });
        let path = format!("/product/domain/{}/dns", path_escape(domain_id));
        self.request::<serde_json::Value>("POST", &path, None, Some(serde_json::to_vec(&payload).unwrap()), None)
            .await
    }

    async fn modify(&self, domain_id: &str, record: &RainyunRecord, domain: &mut Domain, ip_addr: &str) {
        if record.value == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let mut record = record.clone();
        record.value = ip_addr.to_string();
        record.ttl = self.ttl;

        match self.patch_record(domain_id, &record).await {
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

    async fn patch_record(&self, domain_id: &str, record: &RainyunRecord) -> Result<(), String> {
        let payload = serde_json::json!({
            "host": record.host,
            "line": record.line,
            "level": record.level,
            "ttl": record.ttl,
            "type": record.record_type,
            "value": record.value,
            "record_id": record.record_id,
        });
        let path = format!("/product/domain/{}/dns", path_escape(domain_id));
        self.request::<serde_json::Value>("PATCH", &path, None, Some(serde_json::to_vec(&payload).unwrap()), None)
            .await
    }

    async fn request<T: serde::de::DeserializeOwned + Default>(
        &self,
        method: &str,
        path: &str,
        query: Option<&BTreeMap<String, String>>,
        body: Option<Vec<u8>>,
        result: Option<&mut T>,
    ) -> Result<(), String> {
        let mut url = format!("{}{}", RAINYUN_ENDPOINT, path);
        if let Some(q) = query {
            let qs: Vec<String> = q
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            url = format!("{}?{}", url, qs.join("&"));
        }

        let mut builder = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), &url);
        builder = builder.header("x-api-key", &self.dns_conf.DNS.Secret);
        if method == "POST" || method == "PATCH" || method == "PUT" {
            builder = builder.header("Content-Type", "application/json");
        }
        if let Some(b) = body {
            builder = builder.body(b);
        }

        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("返回内容: {} ,返回状态码: {}", text, status.as_u16()));
        }

        let api_resp: RainyunResp = serde_json::from_str(&text)
            .map_err(|e| format!("parse error: {} body: {}", e, text))?;
        if api_resp.code != 200 {
            if !api_resp.message.is_empty() {
                return Err(api_resp.message);
            }
            return Err(format!("Rainyun API error, code={}", api_resp.code));
        }

        if result.is_none() {
            return Ok(());
        }

        let data = api_resp.data.unwrap_or(serde_json::Value::Null);
        let r = result.unwrap();
        *r = if data.is_null() {
            T::default()
        } else {
            serde_json::from_value(data).map_err(|e| format!("parse error: {}", e))?
        };
        Ok(())
    }
}

fn path_escape(s: &str) -> String {
    percent_encoding::percent_encode(s.as_bytes(), percent_encoding::NON_ALPHANUMERIC).to_string()
}

#[async_trait]
impl crate::engine::DnsProvider for Rainyun {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
