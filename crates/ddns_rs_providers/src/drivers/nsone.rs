use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const NSONE_API_ENDPOINT: &str = "https://api.nsone.net/v1/zones";

pub struct NSOne {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct NSOneZone {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct NSOneRecordAnswer {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    answer: Vec<String>,
    #[serde(default)]
    id: String,
}

#[derive(serde::Deserialize, Debug)]
struct NSOneRecordResponse {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    answers: Vec<NSOneRecordAnswer>,
}

#[derive(serde::Serialize)]
struct NSOneRecordRequest {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    answers: Vec<NSOneRecordAnswer>,
    domain: String,
    ttl: i32,
    #[serde(rename = "type")]
    record_type: String,
    zone: String,
}

impl NSOne {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = if dns_conf.TTL.is_empty() {
            60
        } else {
            dns_conf.TTL.parse::<i32>().unwrap_or(3600)
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
            match self.get_zone(&domain).await {
                Ok(_) => {}
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            }

            let existing_record = match self.get_record(&domain, record_type).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };

            if let Some(record) = existing_record {
                self.update_record(&mut domain, record_type, &ip_addr, record).await;
            } else {
                self.create_record(&mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn get_zone(&self, domain: &Domain) -> Result<NSOneZone, String> {
        let url = format!("{}/{}?records=false", NSONE_API_ENDPOINT, domain.domain_name);
        self.request::<NSOneZone, ()>("GET", &url, None).await
    }

    async fn get_record(&self, domain: &Domain, record_type: &str) -> Result<Option<NSOneRecordResponse>, String> {
        let url = format!(
            "{}/{}/{}/{}?records=false",
            NSONE_API_ENDPOINT,
            domain.domain_name,
            domain.full_domain(),
            record_type
        );
        match self.request::<NSOneRecordResponse, ()>("GET", &url, None).await {
            Ok(result) => {
                if !result.answers.is_empty() {
                    Ok(Some(result))
                } else {
                    Ok(None)
                }
            }
            Err(e) => Err(e),
        }
    }

    async fn create_record(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let record_name = domain.full_domain();
        let request = NSOneRecordRequest {
            answers: vec![NSOneRecordAnswer {
                answer: vec![ip_addr.to_string()],
                id: String::new(),
            }],
            domain: record_name.clone(),
            ttl: self.ttl,
            record_type: record_type.to_string(),
            zone: domain.domain_name.clone(),
        };
        let url = format!(
            "{}/{}/{}/{}",
            NSONE_API_ENDPOINT,
            domain.domain_name,
            record_name,
            record_type
        );
        match self
            .request::<NSOneRecordResponse, NSOneRecordRequest>("PUT", &url, Some(&request))
            .await
        {
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

    async fn update_record(
        &self,
        domain: &mut Domain,
        record_type: &str,
        ip_addr: &str,
        existing_record: NSOneRecordResponse,
    ) {
        if !existing_record.answers.is_empty() && !existing_record.answers[0].answer.is_empty() {
            if existing_record.answers[0].answer[0] == ip_addr {
                ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                return;
            }
        }

        let record_name = domain.full_domain();
        let request = NSOneRecordRequest {
            answers: vec![NSOneRecordAnswer {
                answer: vec![ip_addr.to_string()],
                id: String::new(),
            }],
            domain: record_name.clone(),
            ttl: self.ttl,
            record_type: record_type.to_string(),
            zone: domain.domain_name.clone(),
        };
        let url = format!(
            "{}/{}/{}/{}",
            NSONE_API_ENDPOINT,
            domain.domain_name,
            record_name,
            record_type
        );
        match self
            .request::<NSOneRecordResponse, NSOneRecordRequest>("POST", &url, Some(&request))
            .await
        {
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

    async fn request<T: serde::de::DeserializeOwned, D: serde::Serialize>(
        &self,
        method: &str,
        url: &str,
        data: Option<&D>,
    ) -> Result<T, String> {
        let mut req = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url);
        req = req.header("X-NSONE-Key", &self.dns_conf.DNS.Secret);
        req = req.header("Content-Type", "application/json");
        if let Some(d) = data {
            req = req.body(serde_json::to_string(d).unwrap());
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("返回内容: {} ,返回状态码: {}", text, status.as_u16()));
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }
}

#[async_trait]
impl crate::engine::DnsProvider for NSOne {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
