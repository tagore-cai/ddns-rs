use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const DEFAULT_ENDPOINT: &str = "https://dnsmgr.example.com";

pub struct HiPMDnsMgr {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    last_ipv4: String,
    last_ipv6: String,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct ApiResponse {
    code: i32,
    data: serde_json::Value,
    msg: String,
}

#[derive(serde::Deserialize, Debug)]
struct DnsMgrDomain {
    id: i32,
    name: String,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    id: String,
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    value: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordList {
    total: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    list: Vec<Record>,
}

impl HiPMDnsMgr {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
        let last_ipv4 = ipv4_cache.addr.clone();
        let last_ipv6 = ipv6_cache.addr.clone();
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
            last_ipv4,
            last_ipv6,
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
        }
    }

    fn base_url(&self) -> String {
        if self.dns_conf.DNS.ID.is_empty() {
            DEFAULT_ENDPOINT.to_string()
        } else {
            self.dns_conf.DNS.ID.clone()
        }
    }

    fn api_token(&self) -> Result<String, String> {
        if self.dns_conf.DNS.Secret.is_empty() {
            Err("API token cannot be empty".to_string())
        } else {
            Ok(self.dns_conf.DNS.Secret.clone())
        }
    }

    fn build_url(base: &str, path: &str) -> String {
        let base = base.trim_end_matches('/');
        let base = base.trim_end_matches("/api");
        let normalized = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };
        format!("{}/api{}", base, normalized)
    }

    async fn request(
        &self,
        base: &str,
        token: &str,
        method: &str,
        path: &str,
        body: Option<&serde_json::Value>,
    ) -> Result<ApiResponse, String> {
        let url = Self::build_url(base, path);
        let mut builder = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url);
        builder = builder
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", token));
        if let Some(b) = body {
            builder = builder.body(b.to_string());
        }
        let resp = builder.send().await.map_err(|e| e.to_string())?;
        let text = resp.text().await.map_err(|e| e.to_string())?;
        serde_json::from_str(&text).map_err(|e| format!("parse error: {} body: {}", e, text))
    }

    fn parse_domains(data: &serde_json::Value) -> Vec<DnsMgrDomain> {
        match data {
            serde_json::Value::Array(_) => serde_json::from_value::<Vec<DnsMgrDomain>>(data.clone()).unwrap_or_default(),
            serde_json::Value::Object(map) => {
                if let Some(list) = map.get("list") {
                    serde_json::from_value(list.clone()).unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    async fn get_domain_id(&self, base: &str, token: &str, domain_name: &str) -> Result<i32, String> {
        // Method 1: keyword direct query
        let path = format!("/domains?page=1&pageSize=1&keyword={}", domain_name);
        let api_resp = self.request(base, token, "GET", &path, None).await?;
        if api_resp.code != 0 {
            return Err(format!("API error: {}", api_resp.msg));
        }
        let domains = Self::parse_domains(&api_resp.data);
        for d in &domains {
            if d.name == domain_name {
                return Ok(d.id);
            }
        }

        // Method 2: paginated list fallback
        const PAGE_SIZE: i32 = 100;
        let mut current_page = 1;
        loop {
            let path = format!("/domains?page={}&pageSize={}", current_page, PAGE_SIZE);
            let api_resp = self.request(base, token, "GET", &path, None).await?;
            if api_resp.code != 0 {
                return Err(format!("paginated query API error at page {}: {}", current_page, api_resp.msg));
            }
            let page_domains = Self::parse_domains(&api_resp.data);
            let total = api_resp
                .data
                .get("total")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;

            for d in &page_domains {
                if d.name == domain_name {
                    return Ok(d.id);
                }
            }
            if page_domains.len() < PAGE_SIZE as usize || (total > 0 && current_page * PAGE_SIZE >= total) {
                break;
            }
            current_page += 1;
            if current_page > 10 {
                break;
            }
        }
        Err(format!("domain {} not found", domain_name))
    }

    async fn get_record(
        &self,
        base: &str,
        token: &str,
        domain_id: i32,
        sub_domain: &str,
        record_type: &str,
    ) -> Result<Option<Record>, String> {
        const PAGE_SIZE: i32 = 100;
        let mut current_page = 1;
        loop {
            let path = format!(
                "/domains/{}/records?page={}&pageSize={}&subdomain={}&type={}",
                domain_id, current_page, PAGE_SIZE, sub_domain, record_type
            );
            let api_resp = self.request(base, token, "GET", &path, None).await?;
            if api_resp.code != 0 {
                return Err(format!("paginated record query API error at page {}: {}", current_page, api_resp.msg));
            }
            let record_list: RecordList = serde_json::from_value(api_resp.data).map_err(|e| e.to_string())?;
            for r in &record_list.list {
                if r.name == sub_domain && r.record_type == record_type {
                    return Ok(Some(r.clone()));
                }
            }
            if record_list.list.len() < PAGE_SIZE as usize
                || (record_list.total > 0 && current_page * PAGE_SIZE >= record_list.total)
            {
                break;
            }
            current_page += 1;
            if current_page > 10 {
                break;
            }
        }
        Ok(None)
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        // Prevent duplicate webhook notifications.
        if record_type == "A" {
            if self.last_ipv4 == ip_addr {
                ddns_rs_core::log_msg!("你的IPv4未变化, 未触发 %s 请求", "HiPMDnsMgr");
                return;
            }
        } else if self.last_ipv6 == ip_addr {
            ddns_rs_core::log_msg!("你的IPv6未变化, 未触发 %s 请求", "HiPMDnsMgr");
            return;
        }

        for mut domain in domains {
            let result = self.update_record(&domain, &ip_addr, record_type).await;
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
    }

    async fn update_record(&self, domain: &Domain, ip_addr: &str, record_type: &str) -> Result<(), String> {
        let base = self.base_url();
        let token = self.api_token()?;

        let domain_id = self.get_domain_id(&base, &token, &domain.domain_name).await?;
        let record = self.get_record(&base, &token, domain_id, &domain.sub_domain, record_type).await?;

        let ttl: i32 = self.ttl.parse().unwrap_or(600);

        if let Some(record) = record {
            self.update_existing_record(&base, &token, domain_id, &record.id, &domain.sub_domain, record_type, ip_addr, ttl).await
        } else {
            self.create_record(&base, &token, domain_id, &domain.sub_domain, record_type, ip_addr, ttl).await
        }
    }

    async fn create_record(
        &self,
        base: &str,
        token: &str,
        domain_id: i32,
        name: &str,
        record_type: &str,
        value: &str,
        ttl: i32,
    ) -> Result<(), String> {
        let path = format!("/domains/{}/records", domain_id);
        let mut body = serde_json::json!({
            "name": name,
            "type": record_type,
            "value": value,
            "ttl": ttl,
            "line": "0",
        });
        if record_type == "MX" {
            body["mx"] = serde_json::Value::from(10);
        }
        let api_resp = self.request(base, token, "POST", &path, Some(&body)).await?;
        if api_resp.code != 0 {
            return Err(format!("API error: {}", api_resp.msg));
        }
        Ok(())
    }

    async fn update_existing_record(
        &self,
        base: &str,
        token: &str,
        domain_id: i32,
        record_id: &str,
        name: &str,
        record_type: &str,
        value: &str,
        ttl: i32,
    ) -> Result<(), String> {
        let path = format!("/domains/{}/records/{}", domain_id, record_id);
        let mut body = serde_json::json!({
            "name": name,
            "type": record_type,
            "value": value,
            "ttl": ttl,
            "line": "0",
        });
        if record_type == "MX" {
            body["mx"] = serde_json::Value::from(10);
        }
        let api_resp = self.request(base, token, "PUT", &path, Some(&body)).await?;
        if api_resp.code != 0 {
            return Err(format!("API error: {}", api_resp.msg));
        }
        Ok(())
    }
}

#[async_trait]
impl crate::engine::DnsProvider for HiPMDnsMgr {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
