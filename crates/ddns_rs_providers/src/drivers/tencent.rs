use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use serde_json::json;

const ENDPOINT: &str = "https://dnspod.tencentcloudapi.com";
const VERSION: &str = "2021-03-23";

pub struct TencentCloud {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    #[serde(rename = "RecordId")]
    record_id: i64,
    #[serde(rename = "Value")]
    value: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordListResp {
    #[serde(rename = "Response")]
    response: ListResponse,
}

#[derive(serde::Deserialize, Debug)]
struct ListResponse {
    #[serde(rename = "RecordCountInfo")]
    record_count_info: RecordCountInfo,
    #[serde(rename = "RecordList")]
    record_list: Option<Vec<Record>>,
    #[serde(rename = "Error")]
    error: Option<ApiError>,
}

#[derive(serde::Deserialize, Debug)]
struct RecordCountInfo {
    #[serde(rename = "TotalCount")]
    total_count: i32,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResp {
    #[serde(rename = "Response")]
    response: StatusResponse,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResponse {
    #[serde(rename = "Error")]
    error: Option<ApiError>,
}

#[derive(serde::Deserialize, Debug)]
struct ApiError {
    #[serde(rename = "Code")]
    code: String,
    #[serde(rename = "Message")]
    message: String,
}

impl TencentCloud {
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

    fn record_line(&self, domain: &Domain) -> String {
        domain
            .custom_params()
            .get("RecordLine")
            .cloned()
            .unwrap_or_else(|| "默认".to_string())
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            match self.get_record_list(&domain, record_type).await {
                Ok(result) => {
                    if result.response.record_count_info.total_count > 0 {
                        let records_vec = result.response.record_list.unwrap_or_default();
                        let mut record_selected = records_vec[0].clone();
                        if let Some(rid) = domain.custom_params().get("RecordId") {
                            if let Ok(rid_num) = rid.parse::<i64>() {
                                for r in &records_vec {
                                    if r.record_id == rid_num {
                                        record_selected = r.clone();
                                    }
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
        let payload = json!({
            "Domain": domain.domain_name,
            "SubDomain": domain.sub_domain(),
            "RecordType": record_type,
            "RecordLine": self.record_line(domain),
            "Value": ip_addr,
            "TTL": self.ttl,
        });
        match self.request::<StatusResp>("CreateRecord", &payload.to_string()).await {
            Ok(result) => {
                if result.response.error.is_none() {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    let err = result.response.error.unwrap();
                    ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), err.message);
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

        let payload = json!({
            "Domain": domain.domain_name,
            "SubDomain": domain.sub_domain(),
            "RecordType": record_type,
            "RecordLine": self.record_line(domain),
            "Value": ip_addr,
            "TTL": self.ttl,
            "RecordId": record.record_id,
        });
        match self.request::<StatusResp>("ModifyRecord", &payload.to_string()).await {
            Ok(result) => {
                if result.response.error.is_none() {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    let err = result.response.error.unwrap();
                    ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), err.message);
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
        let payload = json!({
            "Domain": domain.domain_name,
            "Subdomain": domain.sub_domain(),
            "RecordType": record_type,
            "RecordLine": self.record_line(domain),
        });
        self.request("DescribeRecordList", &payload.to_string()).await
    }

    async fn request<T: serde::de::DeserializeOwned>(&self, action: &str, payload: &str) -> Result<T, String> {
        let signer = ddns_rs_core::signer::TencentSigner::new(&self.dns_conf.DNS.ID, &self.dns_conf.DNS.Secret);
        let (authorization, host, action, timestamp) = signer.sign("dnspod", action, payload);

        let resp = self
            .http_client
            .post(ENDPOINT)
            .header("Content-Type", "application/json")
            .header("X-TC-Version", VERSION)
            .header("Authorization", authorization)
            .header("Host", host)
            .header("X-TC-Action", action)
            .header("X-TC-Timestamp", timestamp)
            .body(payload.to_string())
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
impl crate::engine::DnsProvider for TencentCloud {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
