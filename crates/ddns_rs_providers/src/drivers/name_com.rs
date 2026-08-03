use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD as B64, Engine};

pub struct NameCom {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
}

#[derive(serde::Serialize)]
struct NameComRecord {
    ttl: i32,
    #[serde(rename = "type")]
    record_type: String,
    answer: String,
    host: String,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct NameComRecordResp {
    #[serde(default)]
    ttl: i32,
    #[serde(rename = "type", default)]
    record_type: String,
    #[serde(default)]
    answer: String,
    #[serde(default)]
    domain_name: String,
    #[serde(default)]
    fqdn: String,
    #[serde(default)]
    host: String,
    #[serde(default)]
    id: i32,
    #[serde(default)]
    priority: i32,
}

#[derive(serde::Deserialize, Debug)]
struct NameComRecordListResp {
    #[serde(default)]
    total_count: i32,
    #[serde(default)]
    from: i32,
    #[serde(default)]
    to: i32,
    #[serde(default)]
    records: Option<Vec<NameComRecordResp>>,
    #[serde(default)]
    last_page: i32,
    #[serde(default)]
    next_page: i32,
}

impl NameCom {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let domains = Domains {
            ipv4_cache,
            ipv6_cache,
            ..Default::default()
        };
        let ttl = if dns_conf.TTL.is_empty() {
            "300".to_string()
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
            let resp = match self.get_record_list(&domain).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            let mut resp4_type_records: Vec<NameComRecordResp> =
                Vec::with_capacity(resp.total_count as usize);
            if resp.total_count > 0 {
                if let Some(records) = resp.records {
                    for r in records {
                        if r.record_type == record_type && r.host == domain.sub_domain {
                            resp4_type_records.push(r);
                        }
                    }
                }
            }

            if !resp4_type_records.is_empty() {
                for r in resp4_type_records {
                    if let Err(_) = self.update_record(r, &mut domain, &ip_addr, record_type).await {
                        domain.update_status = UpdateStatus::Failed;
                        return;
                    }
                }
            } else {
                if let Err(_) = self.create_record(&mut domain, record_type, &ip_addr).await {
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            }
        }
    }

    async fn get_record_list(&self, domain: &Domain) -> Result<NameComRecordListResp, String> {
        let url = format!("https://api.name.com/core/v1/domains/{}/records", domain.domain_name);
        self.request::<NameComRecordListResp, ()>("GET", &url, None).await
    }

    async fn create_record(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) -> Result<(), String> {
        let ttl = match self.ttl.parse::<i32>() {
            Ok(i) => i,
            Err(e) => return Err(e.to_string()),
        };
        let resq = NameComRecord {
            ttl,
            record_type: record_type.to_string(),
            answer: ip_addr.to_string(),
            host: domain.sub_domain.clone(),
        };
        let url = format!("https://api.name.com/core/v1/domains/{}/records", domain.domain_name);
        match self.request::<serde_json::Value, NameComRecord>("POST", &url, Some(&resq)).await {
            Ok(_) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                Ok(())
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                Err(e)
            }
        }
    }

    async fn update_record(
        &self,
        mut record: NameComRecordResp,
        domain: &mut Domain,
        ip_addr: &str,
        record_type: &str,
    ) -> Result<(), String> {
        if record.answer == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return Ok(());
        }
        record.answer = ip_addr.to_string();
        record.record_type = record_type.to_string();
        let url = format!("https://api.name.com/core/v1/domains/{}/records/{}", domain.domain_name, record.id);
        match self.request::<serde_json::Value, NameComRecordResp>("PUT", &url, Some(&record)).await {
            Ok(_) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                Ok(())
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                Err(e)
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
        let auth = B64.encode(format!("{}:{}", self.dns_conf.DNS.ID, self.dns_conf.DNS.Secret));
        req = req.header("Authorization", format!("Basic {}", auth));
        if method.eq_ignore_ascii_case("POST") || method.eq_ignore_ascii_case("PUT") {
            req = req.header("Content-Type", "application/json");
        }
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
impl crate::engine::DnsProvider for NameCom {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
