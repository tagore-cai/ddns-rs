use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const ZONES_API: &str = "https://api.cloudflare.com/client/v4/zones";

pub struct Cloudflare {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct Zone {
    id: String,
    name: String,
}

#[derive(serde::Deserialize, Debug)]
struct ZonesResp {
    success: bool,
    messages: Vec<String>,
    result: Option<Vec<Zone>>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct Record {
    id: String,
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    content: String,
    proxied: bool,
    ttl: i32,
    #[serde(default)]
    comment: Option<String>,
}

#[derive(serde::Deserialize, Debug)]
struct RecordsResp {
    success: bool,
    messages: Vec<String>,
    result: Option<Vec<Record>>,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResp {
    success: bool,
    messages: Vec<String>,
}

impl Cloudflare {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = dns_conf.TTL.parse::<i32>().unwrap_or(1);
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
            let zones = match self.get_zones(&domain).await {
                Ok(z) => z,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };
            let zones_result = zones.result.unwrap_or_default();
            if zones_result.is_empty() {
                ddns_rs_core::log_msg!("在DNS服务商中未找到根域名: %s", domain.domain_name);
                domain.update_status = UpdateStatus::Failed;
                continue;
            }

            let zone_id = zones_result[0].id.clone();
            let params = domain.custom_params();
            let mut query = format!("type={}&name={}&per_page=50", record_type, domain.to_ascii());
            if let Some(c) = params.get("comment") {
                if !c.is_empty() {
                    query = format!("{}&comment={}", query, c);
                }
            }

            let records = match self
                .request::<RecordsResp, ()>("GET", &format!("{}/{}/dns_records?{}", ZONES_API, zone_id, query), None)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }
            };
            if !records.success {
                ddns_rs_core::log_msg!("查询域名信息发生异常! %s", records.messages.join(", "));
                domain.update_status = UpdateStatus::Failed;
                continue;
            }

            let records_result = records.result.unwrap_or_default();
            if !records_result.is_empty() {
                self.modify(records_result, &zone_id, &mut domain, &ip_addr).await;
            } else {
                self.create(&zone_id, &mut domain, record_type, &ip_addr).await;
            }
        }
    }

    async fn create(&self, zone_id: &str, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let params = domain.custom_params();
        let record = Record {
            id: String::new(),
            name: domain.to_ascii(),
            record_type: record_type.to_string(),
            content: ip_addr.to_string(),
            proxied: params.get("proxied").map(|p| p == "true").unwrap_or(false),
            ttl: self.ttl,
            comment: params.get("comment").cloned(),
        };
        match self
            .request::<StatusResp, Record>("POST", &format!("{}/{}/dns_records", ZONES_API, zone_id), Some(&record))
            .await
        {
            Ok(status) => {
                if status.success {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), status.messages.join(", "));
                    domain.update_status = UpdateStatus::Failed;
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
            }
        }
    }

    async fn modify(&self, records: Vec<Record>, zone_id: &str, domain: &mut Domain, ip_addr: &str) {
        let params = domain.custom_params();
        for mut record in records {
            if record.content == ip_addr {
                ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                continue;
            }
            record.content = ip_addr.to_string();
            record.ttl = self.ttl;
            if params.contains_key("proxied") {
                record.proxied = params.get("proxied").map(|p| p == "true").unwrap_or(false);
            }
            match self
                .request::<StatusResp, Record>("PUT", &format!("{}/{}/dns_records/{}", ZONES_API, zone_id, record.id), Some(&record))
                .await
            {
                Ok(status) => {
                    if status.success {
                        ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                        domain.update_status = UpdateStatus::Success;
                    } else {
                        ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), status.messages.join(", "));
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

    async fn get_zones(&self, domain: &Domain) -> Result<ZonesResp, String> {
        let query = format!("name={}&status=active&per_page=50", domain.domain_name);
        self.request::<ZonesResp, ()>("GET", &format!("{}?{}", ZONES_API, query), None)
            .await
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
        req = req.header("Authorization", format!("Bearer {}", self.dns_conf.DNS.Secret));
        req = req.header("Content-Type", "application/json");
        if let Some(d) = data {
            req = req.body(serde_json::to_string(d).unwrap());
        }
        let resp = req.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            ddns_rs_core::log_msg!("返回内容: %s ,返回状态码: %d", text, status.as_u16());
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Cloudflare {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_zones_resp() {
        let s: ZonesResp =
            serde_json::from_str(r#"{"success":true,"messages":[],"result":[{"id":"z1","name":"example.com"}]}"#)
                .unwrap();
        assert!(s.success);
        assert_eq!(s.result.as_ref().unwrap().len(), 1);
        assert_eq!(s.result.as_ref().unwrap()[0].name, "example.com");
    }

    #[test]
    fn test_parse_zones_resp_null_result() {
        // Cloudflare returns "result":null when a zone is not found.
        let s: ZonesResp =
            serde_json::from_str(r#"{"success":true,"messages":[],"result":null}"#).unwrap();
        assert!(s.success);
        assert!(s.result.is_none(), "null result must parse as None");
    }

    #[test]
    fn test_parse_records_resp() {
        let s: RecordsResp = serde_json::from_str(
            r#"{"success":true,"messages":[],"result":[{"id":"r1","name":"www","type":"A","content":"1.2.3.4","proxied":false,"ttl":1,"comment":""}]}"#,
        )
        .unwrap();
        assert!(s.success);
        let rec = &s.result.as_ref().unwrap()[0];
        assert_eq!(rec.record_type, "A");
        assert_eq!(rec.content, "1.2.3.4");
    }

    #[test]
    fn test_parse_records_resp_null_result() {
        // No records -> result is null.
        let s: RecordsResp = serde_json::from_str(r#"{"success":true,"messages":[],"result":null}"#).unwrap();
        assert!(s.result.is_none());
    }

    #[test]
    fn test_parse_records_resp_null_comment() {
        // Cloudflare returns "comment":null for records without a comment.
        let s: RecordsResp = serde_json::from_str(
            r#"{"success":true,"messages":[],"result":[{"id":"r1","name":"www","type":"AAAA","content":"::1","proxiable":true,"proxied":false,"ttl":1,"settings":{},"meta":{},"comment":null,"tags":[],"created_on":"2026-07-17T09:08:34.312763Z","modified_on":"2026-08-02T14:13:50.864713Z"}]}"#,
        )
        .unwrap();
        assert!(s.success);
        let rec = &s.result.as_ref().unwrap()[0];
        assert_eq!(rec.comment, None, "null comment must parse as None");
    }

    #[test]
    fn test_parse_records_resp_missing_comment() {
        // Older/newer API may omit comment entirely; must also parse.
        let s: RecordsResp = serde_json::from_str(
            r#"{"success":true,"messages":[],"result":[{"id":"r1","name":"www","type":"A","content":"1.2.3.4","proxied":false,"ttl":1}]}"#,
        )
        .unwrap();
        let rec = &s.result.as_ref().unwrap()[0];
        assert_eq!(rec.comment, None);
    }

    #[test]
    fn test_parse_status_resp() {
        let s: StatusResp = serde_json::from_str(r#"{"success":true,"messages":[],"result":{"id":"r1"}}"#).unwrap();
        assert!(s.success);
    }
}
