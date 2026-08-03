use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};

const RECORD_LIST: &str = "http://api.dns.la/api/recordList";
const RECORD_MODIFY: &str = "http://api.dns.la/api/record";
const RECORD_CREATE: &str = "http://api.dns.la/api/record";

pub struct Dnsla {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    id: String,
    host: String,
    #[serde(rename = "type")]
    record_type: i32,
    data: String,
}

#[derive(serde::Deserialize, Debug)]
struct ListResp {
    code: i32,
    msg: String,
    data: ListData,
}

#[derive(serde::Deserialize, Debug)]
struct ListData {
    total: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    results: Vec<Record>,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResp {
    code: i32,
    msg: String,
}

impl Dnsla {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = dns_conf.TTL.parse::<i32>().unwrap_or(600);
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            ttl,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    fn basic_auth(&self) -> String {
        let cred = format!("{}:{}", self.dns_conf.DNS.ID, self.dns_conf.DNS.Secret);
        format!("Basic {}", B64.encode(cred.as_bytes()))
    }

    fn record_type_int(record_type: &str) -> i32 {
        if record_type == "AAAA" {
            28
        } else {
            1
        }
    }

    async fn request_raw(&self, method: &str, url: &str, body: Option<&serde_json::Value>) -> Result<String, String> {
        let mut builder = self.http_client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            url,
        );
        builder = builder.header("Authorization", self.basic_auth());
        if method == "POST" || method == "PUT" {
            builder = builder.header("Content-Type", "application/json;charset=utf-8");
            if let Some(b) = body {
                builder = builder.body(b.to_string());
            }
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("dnsla 请求失败，状态码: {}, 响应: {}", status.as_u16(), text));
        }
        Ok(text)
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        url: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<T, String> {
        let text = self.request_raw(method, url, body).await?;
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    async fn get_record_list(&self, domain: &Domain, record_type: &str) -> Result<ListResp, String> {
        let mut params = domain.custom_params();
        params.insert("domain".into(), domain.domain_name.clone());
        params.insert("host".into(), domain.sub_domain());
        params.insert("type".into(), Self::record_type_int(record_type).to_string());
        params.insert("pageIndex".into(), "1".into());
        params.insert("pageSize".into(), "999".into());
        let qs: Vec<String> = params.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        self.request("GET", &format!("{}?{}", RECORD_LIST, qs.join("&")), None).await
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let json_result = match self.get_record_list(&domain, record_type).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            if json_result.data.total > 0 {
                let mut record_selected = json_result.data.results[0].clone();
                if let Some(id) = domain.custom_params().get("id") {
                    for r in &json_result.data.results {
                        if &r.id == id {
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
        let body = serde_json::json!({
            "Domain": domain.domain_name,
            "Host": domain.sub_domain(),
            "Type": Self::record_type_int(record_type),
            "Data": ip_addr,
            "TTL": self.ttl,
        });
        let result: Result<StatusResp, String> = self.request("POST", RECORD_CREATE, Some(&body)).await;
        match result {
            Ok(r) if r.code == 200 => {
                ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            }
            Ok(r) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), r.msg);
                domain.update_status = UpdateStatus::Failed;
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn modify(&self, record: Record, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        if record.data == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }
        let body = serde_json::json!({
            "Id": record.id,
            "Host": domain.sub_domain(),
            "Type": Self::record_type_int(record_type),
            "Data": ip_addr,
            "TTL": self.ttl,
        });
        let result: Result<StatusResp, String> = self.request("PUT", RECORD_MODIFY, Some(&body)).await;
        match result {
            Ok(r) if r.code == 200 => {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                domain.update_status = UpdateStatus::Success;
            }
            Ok(r) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), r.msg);
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
impl crate::engine::DnsProvider for Dnsla {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
