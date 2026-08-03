use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const ENDPOINT: &str = "https://dynv6.com";

pub struct Dynv6 {
    dns_conf: DnsConfig,
    domains: Domains,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone, serde::Serialize)]
struct Zone {
    id: u64,
    name: String,
    ipv4address: String,
    ipv6prefix: String,
}

#[derive(serde::Deserialize, Debug, Clone, serde::Serialize)]
struct Record {
    id: u64,
    zone_id: Option<u64>,
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    data: String,
}

impl Dynv6 {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        Self {
            dns_conf: dns_conf.clone(),
            domains,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        url: &str,
        data: Option<&serde_json::Value>,
    ) -> Result<T, String> {
        let mut builder = self.http_client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            url,
        );
        builder = builder.header("Authorization", format!("Bearer {}", self.dns_conf.DNS.Secret));
        builder = builder.header("Content-Type", "application/json");
        if let Some(d) = data {
            builder = builder.body(d.to_string());
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            ddns_rs_core::log_msg!("返回内容: %s ,返回状态码: %d", text, status.as_u16());
        }
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    async fn find_zone(&self, domain: &Domain) -> Result<(bool, Zone, bool), String> {
        let zones: Vec<Zone> = self.request("GET", &format!("{}/api/v2/zones", ENDPOINT), None).await?;
        for z in zones {
            if domain.display().ends_with(&z.name) {
                let is_main = domain.display() == z.name;
                return Ok((true, z, is_main));
            }
        }
        Ok((false, Zone { id: 0, name: String::new(), ipv4address: String::new(), ipv6prefix: String::new() }, false))
    }

    async fn find_record(&self, domain: &Domain, zone_id: &str, record_type: &str) -> Result<(bool, Record), String> {
        let records: Vec<Record> = self
            .request("GET", &format!("{}/api/v2/zones/{}/records", ENDPOINT, zone_id), None)
            .await?;
        for r in records {
            if r.name == domain.sub_domain && r.record_type == record_type {
                return Ok((true, r));
            }
        }
        Ok((false, Record { id: 0, zone_id: None, name: String::new(), record_type: String::new(), data: String::new() }))
    }

    fn process_sub_domain(domain: &mut Domain, zone: &Zone) -> bool {
        let sub_domain_len = domain.display().len() as i64 - zone.name.len() as i64 - 1;
        if sub_domain_len <= 0 {
            return false;
        }
        let sub_domain = domain.display()[..sub_domain_len as usize].to_string();
        domain.domain_name = zone.name.clone();
        domain.sub_domain = sub_domain;
        true
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let (is_find_zone, find_zone, is_main) = match self.find_zone(&domain).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };
            if !is_find_zone {
                ddns_rs_core::log_msg!("在DNS服务商中未找到根域名: %s", domain.display());
                domain.update_status = UpdateStatus::Failed;
                continue;
            }

            let zone_id = find_zone.id.to_string();

            if is_main {
                if (record_type == "A" && find_zone.ipv4address == ip_addr)
                    || (record_type == "AAAA" && find_zone.ipv6prefix == ip_addr)
                {
                    ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                    domain.update_status = UpdateStatus::Nothing;
                } else {
                    self.modify_main(&mut domain, &zone_id, record_type, &ip_addr).await;
                }
            } else {
                if !Self::process_sub_domain(&mut domain, &find_zone) {
                    ddns_rs_core::log_msg!("域名: %s 不正确", domain.display());
                    domain.update_status = UpdateStatus::Failed;
                    continue;
                }

                let (is_find_record, find_record) = match self.find_record(&domain, &zone_id, record_type).await {
                    Ok(r) => r,
                    Err(e) => {
                        ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                        domain.update_status = UpdateStatus::Failed;
                        return;
                    }
                };

                if is_find_record {
                    if find_record.record_type == record_type && find_record.data == ip_addr {
                        ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                        domain.update_status = UpdateStatus::Nothing;
                    } else {
                        self.modify(&mut domain, &zone_id, &find_record, record_type, &ip_addr).await;
                    }
                } else {
                    self.create(&mut domain, &zone_id, record_type, &ip_addr).await;
                }
            }
        }
    }

    async fn modify_main(&self, domain: &mut Domain, zone_id: &str, record_type: &str, ip_addr: &str) {
        let body = if record_type == "A" {
            serde_json::json!({ "ipv4address": ip_addr })
        } else {
            serde_json::json!({ "ipv6prefix": ip_addr })
        };
        let result: Result<Zone, String> = self
            .request("PATCH", &format!("{}/api/v2/zones/{}", ENDPOINT, zone_id), Some(&body))
            .await;
        match result {
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

    async fn create(&self, domain: &mut Domain, zone_id: &str, record_type: &str, ip_addr: &str) {
        let body = serde_json::json!({
            "name": domain.sub_domain,
            "type": record_type,
            "data": ip_addr,
        });
        let result: Result<Record, String> = self
            .request("POST", &format!("{}/api/v2/zones/{}/records", ENDPOINT, zone_id), Some(&body))
            .await;
        match result {
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

    async fn modify(&self, domain: &mut Domain, zone_id: &str, record: &Record, record_type: &str, ip_addr: &str) {
        let body = serde_json::json!({
            "name": record.name,
            "type": record_type,
            "data": ip_addr,
        });
        let result: Result<Record, String> = self
            .request(
                "PATCH",
                &format!("{}/api/v2/zones/{}/records/{}", ENDPOINT, zone_id, record.id),
                Some(&body),
            )
            .await;
        match result {
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
}

#[async_trait]
impl crate::engine::DnsProvider for Dynv6 {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
