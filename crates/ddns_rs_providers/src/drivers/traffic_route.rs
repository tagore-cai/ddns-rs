use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use ddns_rs_core::signer::TrafficRouteSigner;
use async_trait::async_trait;

pub struct TrafficRoute {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Meta {
    #[serde(rename = "ZID")]
    zid: i32,
    #[serde(rename = "RecordID")]
    record_id: String,
    #[serde(rename = "Host")]
    host: String,
    #[serde(rename = "Type")]
    record_type: String,
    #[serde(rename = "Value")]
    value: String,
    #[serde(rename = "TTL")]
    ttl: i32,
    #[serde(rename = "Line")]
    line: String,
}

#[derive(serde::Deserialize, Debug)]
struct Resp {
    #[serde(rename = "ResponseMetadata")]
    response_metadata: ResponseMetadata,
    #[serde(rename = "Result")]
    result: ResultData,
}

#[derive(serde::Deserialize, Debug)]
struct ResponseMetadata {
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

#[derive(serde::Deserialize, Debug)]
struct ResultData {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    zones: Vec<Zone>,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    records: Vec<Meta>,
    #[serde(rename = "RecordID", default)]
    record_id: String,
}

#[derive(serde::Deserialize, Debug)]
struct Zone {
    #[serde(rename = "ZID")]
    zid: i32,
    #[serde(rename = "ZoneName")]
    zone_name: String,
}

impl TrafficRoute {
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

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        action: &str,
        query: &[(String, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<T, String> {
        let body_bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
        let signer = TrafficRouteSigner::new(&self.dns_conf.DNS.ID, &self.dns_conf.DNS.Secret);
        let (auth, x_date, x_content_sha256, host, content_type) =
            signer.sign(method, query, action, &body_bytes);

        let mut builder = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), "https://open.volcengineapi.com/");
        let mut q = query.to_vec();
        q.push(("Action".to_string(), action.to_string()));
        q.push(("Version".to_string(), "2018-08-01".to_string()));
        let qs: Vec<String> = q.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        builder = builder.query(&qs.join("&"));
        builder = builder
            .header("Host", host)
            .header("Content-Type", content_type)
            .header("X-Date", x_date)
            .header("X-Content-Sha256", x_content_sha256)
            .header("Authorization", auth);
        if let Some(b) = body {
            builder = builder.body(b.to_string());
        }

        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            ddns_rs_core::log_msg!("返回内容: %s ,返回状态码: %d", text, status.as_u16());
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    async fn get_zid(&self, domain: &Domain) -> Result<i32, String> {
        let resp: Resp = self
            .request("GET", "ListZones", &[("Key".to_string(), domain.domain_name.clone())], None)
            .await?;
        for zone in resp.result.zones {
            if zone.zone_name == domain.domain_name {
                return Ok(zone.zid);
            }
        }
        Err("在DNS服务商中未找到域名".to_string())
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let zone_id = match self.get_zid(&domain).await {
                Ok(z) => z,
                Err(e) => {
                    ddns_rs_core::log_msg!("在DNS服务商中未找到域名: %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };

            let resp: Resp = match self
                .request(
                    "GET",
                    "ListRecords",
                    &[
                        ("ZID".to_string(), zone_id.to_string()),
                        ("Type".to_string(), record_type.to_string()),
                        ("Host".to_string(), domain.sub_domain()),
                        ("SearchMode".to_string(), "exact".to_string()),
                        ("PageNumber".to_string(), "1".to_string()),
                        ("PageSize".to_string(), "500".to_string()),
                    ],
                    None,
                )
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };

            let mut found = false;
            for record in resp.result.records {
                if record.record_type == record_type && record.host == domain.sub_domain() {
                    self.modify(record, &mut domain, &ip_addr).await;
                    found = true;
                    break;
                }
            }
            if !found {
                self.create(zone_id, &mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn create(&self, zone_id: i32, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let body = serde_json::json!({
            "ZID": zone_id,
            "Host": domain.sub_domain(),
            "Type": record_type,
            "Value": ip_addr,
            "TTL": self.ttl,
            "Line": "default",
        });
        let result: Result<Resp, String> = self.request("POST", "CreateRecord", &[], Some(&body)).await;
        match result {
            Ok(r) => {
                if r.response_metadata.error.is_none() {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    let err = r.response_metadata.error.unwrap();
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

    async fn modify(&self, mut record: Meta, domain: &mut Domain, ip_addr: &str) {
        if record.value == ip_addr {
            ddns_rs_core::log_msg!("IP %s 没有变化，域名 %s", ip_addr, domain.display());
            domain.update_status = UpdateStatus::Nothing;
            return;
        }
        record.value = ip_addr.to_string();
        record.ttl = self.ttl;
        let body = serde_json::json!({
            "ZID": record.zid,
            "RecordID": record.record_id,
            "Host": record.host,
            "Type": record.record_type,
            "Value": record.value,
            "TTL": record.ttl,
            "Line": record.line,
        });
        let result: Result<Resp, String> = self.request("POST", "UpdateRecord", &[], Some(&body)).await;
        match result {
            Ok(r) => {
                if r.response_metadata.error.is_none() {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    let err = r.response_metadata.error.unwrap();
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
}

#[async_trait]
impl crate::engine::DnsProvider for TrafficRoute {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
