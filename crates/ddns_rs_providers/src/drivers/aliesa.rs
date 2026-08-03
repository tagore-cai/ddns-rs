use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{DomainTuple, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::collections::HashMap;

const ENDPOINT: &str = "https://esa.cn-hangzhou.aliyuncs.com/";

pub struct Aliesa {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: String,
    http_client: reqwest::Client,
    site_cache: HashMap<String, Site>,
    domain_cache: HashMap<String, DomainTuple>,
}

#[derive(Debug, Clone)]
struct Site {
    site_id: i64,
    site_name: String,
    access_type: String,
}

#[derive(serde::Deserialize, Debug)]
struct SiteResp {
    total_count: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    sites: Vec<SiteRaw>,
}

#[derive(serde::Deserialize, Debug)]
struct SiteRaw {
    site_id: i64,
    site_name: String,
    access_type: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordResp {
    total_count: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    records: Vec<Record>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Record {
    record_id: i64,
    record_name: String,
    data: RecordData,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct RecordData {
    value: String,
}

#[derive(serde::Deserialize, Debug)]
struct ApiResp {
    #[serde(rename = "Id")]
    origin_pool_id: i64,
    #[serde(rename = "RecordId")]
    record_id: i64,
    #[serde(rename = "RequestId")]
    request_id: String,
}

#[derive(serde::Deserialize, Debug)]
struct OriginPoolResp {
    total_count: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    origin_pools: Vec<OriginPool>,
}

#[derive(serde::Deserialize, Debug)]
struct OriginPool {
    id: i64,
    origins: Vec<serde_json::Map<String, serde_json::Value>>,
}

impl Aliesa {
    pub fn new(dns_conf: &DnsConfig, ipv4_cache: IpCache, ipv6_cache: IpCache) -> Self {
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
            http_client: ddns_rs_core::httpclient::create_http_client_with_interface(&dns_conf.HttpInterface),
            site_cache: HashMap::new(),
            domain_cache: HashMap::new(),
        }
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: &mut BTreeMap<String, String>,
    ) -> Result<T, String> {
        ddns_rs_core::signer::aliyun_sign(
            &self.dns_conf.DNS.ID,
            &self.dns_conf.DNS.Secret,
            params,
            method,
            "2024-09-10",
        );
        let query = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");
        let url = format!("{}?{}", ENDPOINT, query);

        let resp = self
            .http_client
            .request(reqwest::Method::from_bytes(method.as_bytes()).unwrap(), url)
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

    async fn get_site(&mut self, domain_tuple: &DomainTuple) -> Result<Site, String> {
        let domain = &domain_tuple.primary;
        if let Some(site) = self.site_cache.get(&domain.domain_name) {
            return Ok(site.clone());
        }

        // Parse SiteId from custom params
        let site_id_str = domain.custom_params().get("SiteId").cloned().unwrap_or_default();
        if let Ok(site_id) = site_id_str.parse::<i64>() {
            if site_id != 0 {
                let site = Site {
                    site_id,
                    site_name: domain.domain_name.clone(),
                    access_type: "CNAME".to_string(),
                };
                return Ok(site);
            }
        }

        let mut params = BTreeMap::new();
        params.insert("Action".into(), "ListSites".into());
        params.insert("SiteName".into(), domain.domain_name.clone());
        let site_resp: SiteResp = self.request("GET", &mut params).await?;

        if site_resp.sites.is_empty() {
            return Err("no sites".to_string());
        }
        let raw = &site_resp.sites[0];
        let site = Site {
            site_id: raw.site_id,
            site_name: raw.site_name.clone(),
            access_type: raw.access_type.clone(),
        };
        self.site_cache.insert(domain.domain_name.clone(), site.clone());
        Ok(site)
    }

    async fn get_record(&self, site: &Site, domain_tuple: &DomainTuple, record_type: &str) -> Result<Option<Record>, String> {
        let domain = &domain_tuple.primary;
        let mut params = BTreeMap::new();
        params.insert("Action".into(), "ListRecords".into());
        params.insert("SiteId".into(), site.site_id.to_string());
        params.insert("RecordName".into(), domain.display());
        params.insert("Type".into(), record_type.into());
        let record_resp: RecordResp = self.request("GET", &mut params).await?;

        if record_resp.records.is_empty() {
            return Ok(None);
        }

        let record_id = domain.custom_params().get("RecordId").cloned().unwrap_or_default();
        if !record_id.is_empty() {
            for r in &record_resp.records {
                if r.record_id.to_string() == record_id {
                    return Ok(Some(r.clone()));
                }
            }
        }
        Ok(Some(record_resp.records[0].clone()))
    }

    async fn add_update(&mut self, record_type: &str) {
        let tuples: Vec<DomainTuple> = self
            .domain_cache
            .values()
            .filter(|t| t.record_type == record_type)
            .cloned()
            .collect();

        for mut tuple in tuples {
            let site = match self.get_site(&tuple).await {
                Ok(s) => s,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    tuple.set_update_status(UpdateStatus::Failed);
                    return;
                }
            };
            if site.site_id == 0 {
                ddns_rs_core::log_msg!("在DNS服务商中未找到根域名: %s", tuple.primary.domain_name);
                tuple.set_update_status(UpdateStatus::Failed);
                return;
            }

            // Origin pool handling
            let (pool_id, origins) = match self.get_origin_pool(&site, &tuple).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    tuple.set_update_status(UpdateStatus::Failed);
                    return;
                }
            };
            if !origins.is_empty() {
                self.update_origin_pool(&site, &mut tuple, pool_id, origins).await;
                return;
            }

            let record_selected = match self.get_record(&site, &tuple, "A/AAAA").await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    tuple.set_update_status(UpdateStatus::Failed);
                    return;
                }
            };
            if let Some(record) = record_selected {
                if record.record_id != 0 {
                    self.modify(&site, &mut tuple, "A/AAAA", &record).await;
                    continue;
                }
            }
            self.create(&site, &mut tuple, "A/AAAA").await;
        }
    }

    async fn create(&self, site: &Site, tuple: &mut DomainTuple, record_type: &str) {
        let domain = &tuple.primary;
        let ip_addr = tuple.get_ip_addr_pool(",");
        let mut params = domain.custom_params();
        params.insert("Action".into(), "CreateRecord".into());
        params.insert("SiteId".into(), site.site_id.to_string());
        params.insert("RecordName".into(), domain.display());
        params.insert("Type".into(), record_type.into());
        params.insert("Data".into(), format!(r#"{{"Value":"{}"}}"#, ip_addr));
        params.insert("Ttl".into(), self.ttl.clone());

        if site.access_type == "CNAME" && !params.contains_key("Proxied") {
            params.insert("Proxied".into(), "true".into());
        }
        if params.contains_key("Proxied") && !params.contains_key("BizName") {
            params.insert("BizName".into(), "web".into());
        }

        let result: Result<ApiResp, String> = self.request("POST", &mut params).await;
        match result {
            Ok(r) if r.record_id != 0 => {
                ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                tuple.set_update_status(UpdateStatus::Success);
            }
            Ok(_) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), "返回RecordId为空");
                tuple.set_update_status(UpdateStatus::Failed);
            }
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                tuple.set_update_status(UpdateStatus::Failed);
            }
        }
    }

    async fn modify(&self, _site: &Site, tuple: &mut DomainTuple, record_type: &str, record: &Record) {
        let domain = &tuple.primary;
        let ip_addr = tuple.get_ip_addr_pool(",");
        if record.data.value == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let mut params = domain.custom_params();
        params.insert("Action".into(), "UpdateRecord".into());
        params.insert("RecordId".into(), record.record_id.to_string());
        params.insert("Type".into(), record_type.into());
        params.insert("Data".into(), format!(r#"{{"Value":"{}"}}"#, ip_addr));
        params.insert("Ttl".into(), self.ttl.clone());

        let result: Result<ApiResp, String> = self.request("POST", &mut params).await;
        match result {
            Ok(_) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                tuple.set_update_status(UpdateStatus::Success);
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                tuple.set_update_status(UpdateStatus::Failed);
            }
        }
    }

    async fn get_origin_pool(&self, site: &Site, tuple: &DomainTuple) -> Result<(i64, Vec<serde_json::Map<String, serde_json::Value>>), String> {
        let sub = &tuple.primary.sub_domain;
        if !sub.ends_with(".origin-pool") {
            return Ok((0, Vec::new()));
        }
        let name = sub.trim_end_matches(".origin-pool").to_string();

        let mut params = BTreeMap::new();
        params.insert("Action".into(), "ListOriginPools".into());
        params.insert("SiteId".into(), site.site_id.to_string());
        params.insert("Name".into(), name);
        params.insert("MatchType".into(), "exact".into());

        let result: OriginPoolResp = self.request("GET", &mut params).await?;
        if let Some(pool) = result.origin_pools.into_iter().next() {
            Ok((pool.id, pool.origins))
        } else {
            Ok((0, Vec::new()))
        }
    }

    async fn update_origin_pool(&self, site: &Site, tuple: &mut DomainTuple, id: i64, mut origins: Vec<serde_json::Map<String, serde_json::Value>>) {
        let domain = &tuple.primary;
        let ip_addr = tuple.get_ip_addr_pool(",");

        let mut need_update = false;
        let mut count = tuple.domains.len();
        for origin in origins.iter_mut() {
            for (i, d) in tuple.domains.iter().enumerate() {
                let name = d.custom_params().get("Name").cloned().unwrap_or_default();
                let origin_name = origin.get("Name").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if origin_name != name {
                    continue;
                }
                let address = tuple.ip_addrs.get(i).cloned().unwrap_or_default();
                let origin_addr = origin
                    .get("Address")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                if origin_addr != Some(address.clone()) {
                    origin.insert("Address".into(), serde_json::Value::String(address));
                    need_update = true;
                }
                count -= 1;
                break;
            }
        }

        if count > 0 {
            ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), "不支持新增源地址");
            tuple.set_update_status(UpdateStatus::Failed);
            return;
        }
        if !need_update {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let origins_json = serde_json::Value::Array(
            origins.into_iter().map(serde_json::Value::Object).collect(),
        );
        let mut params = domain.custom_params();
        params.insert("Action".into(), "UpdateOriginPool".into());
        params.insert("SiteId".into(), site.site_id.to_string());
        params.insert("Id".into(), id.to_string());
        params.insert("Origins".into(), origins_json.to_string());

        let result: Result<ApiResp, String> = self.request("POST", &mut params).await;
        match result {
            Ok(r) if r.origin_pool_id != 0 => {
                ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                tuple.set_update_status(UpdateStatus::Success);
            }
            Ok(_) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), "返回 OriginPool Id为空");
                tuple.set_update_status(UpdateStatus::Failed);
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                tuple.set_update_status(UpdateStatus::Failed);
            }
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Aliesa {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.site_cache = HashMap::new();
        self.domain_cache = self.domains.get_all_new_ip_result("A/AAAA");
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.add_update("A/AAAA").await;
        self.domains.clone()
    }
}
