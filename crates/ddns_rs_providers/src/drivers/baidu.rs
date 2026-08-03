use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use ddns_rs_core::signer::BaiduSigner;
use async_trait::async_trait;

const ENDPOINT: &str = "https://bcd.baidubce.com";

pub struct BaiduCloud {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    record_id: u64,
    domain: String,
    view: String,
    rdtype: String,
    ttl: i32,
    rdata: String,
    zone_name: String,
    status: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordsResp {
    total_count: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    result: Vec<Record>,
}

impl BaiduCloud {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = dns_conf.TTL.parse::<i32>().unwrap_or(300);
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            ttl,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T, String> {
        let signer = BaiduSigner::new(&self.dns_conf.DNS.ID, &self.dns_conf.DNS.Secret);
        let auth = signer.sign("POST", path);
        let resp = self
            .http_client
            .post(format!("{}{}", ENDPOINT, path))
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .body(body.to_string())
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

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let list_body = serde_json::json!({
                "domain": domain.domain_name,
                "pageNum": 1,
                "pageSize": 1000,
            });
            let records: RecordsResp = match self.request("/v1/domain/resolve/list", &list_body).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            let mut find = false;
            for record in &records.result {
                if record.domain == domain.sub_domain() {
                    self.modify(record.clone(), &mut domain, record_type, &ip_addr).await;
                    find = true;
                    break;
                }
            }
            if !find {
                self.create(&mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let body = serde_json::json!({
            "domain": domain.sub_domain(),
            "rdType": record_type,
            "ttl": self.ttl,
            "rdata": ip_addr,
            "zoneName": domain.domain_name,
        });
        let result: Result<RecordsResp, String> = self.request("/v1/domain/resolve/add", &body).await;
        if result.is_ok() {
            ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
            domain.update_status = UpdateStatus::Success;
        } else {
            ddns_rs_core::log_msg!(
                "新增域名解析 %s 失败! 异常信息: %s",
                domain.display(),
                result.err().unwrap()
            );
            domain.update_status = UpdateStatus::Failed;
        }
    }

    async fn modify(&self, record: Record, domain: &mut Domain, _rd_type: &str, ip_addr: &str) {
        if record.rdata == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }
        let body = serde_json::json!({
            "recordId": record.record_id,
            "domain": record.domain,
            "view": record.view,
            "rdType": record.rdtype,
            "ttl": record.ttl,
            "rdata": ip_addr,
            "zoneName": record.zone_name,
        });
        let result: Result<RecordsResp, String> = self.request("/v1/domain/resolve/edit", &body).await;
        if result.is_ok() {
            ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
            domain.update_status = UpdateStatus::Success;
        } else {
            ddns_rs_core::log_msg!(
                "更新域名解析 %s 失败! 异常信息: %s",
                domain.display(),
                result.err().unwrap()
            );
            domain.update_status = UpdateStatus::Failed;
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for BaiduCloud {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
