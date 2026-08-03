use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, DomainTuple, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::collections::HashMap;

const ENDPOINT: &str = "https://teo.tencentcloudapi.com";
const VERSION: &str = "2022-09-01";
const ORIGIN_RECORD_TYPE: &str = "IP_DOMAIN";

pub struct EdgeOne {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Zone {
    zone_id: String,
    zone_name: String,
}

#[derive(serde::Deserialize, Debug)]
struct ZoneResp {
    response: ZoneResponse,
}

#[derive(serde::Deserialize, Debug)]
struct ZoneResponse {
    total_count: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    zones: Vec<Zone>,
    error: Option<ApiError>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct DnsRecord {
    zone_id: String,
    name: String,
    #[serde(rename = "type")]
    record_type: String,
    content: String,
    location: String,
    ttl: i32,
    #[serde(rename = "RecordId")]
    record_id: String,
    #[serde(rename = "Status")]
    status: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordResp {
    response: RecordResponse,
}

#[derive(serde::Deserialize, Debug)]
struct RecordResponse {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    dns_records: Vec<DnsRecord>,
    total_count: i32,
    error: Option<ApiError>,
}

#[derive(serde::Deserialize, Debug)]
struct ApiError {
    code: String,
    message: String,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResp {
    response: StatusResponse,
}

#[derive(serde::Deserialize, Debug)]
struct StatusResponse {
    error: Option<ApiError>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
struct OriginRecord {
    #[serde(rename = "Record")]
    record: String,
    #[serde(rename = "Type")]
    record_type: String,
    #[serde(rename = "Weight", skip_serializing_if = "is_zero")]
    weight: i32,
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

#[derive(serde::Deserialize, Debug, Clone)]
struct OriginGroup {
    group_id: String,
    name: String,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    records: Vec<OriginRecord>,
}

#[derive(serde::Deserialize, Debug)]
struct OriginGroupResp {
    response: OriginGroupResponse,
}

#[derive(serde::Deserialize, Debug)]
struct OriginGroupResponse {
    error: Option<ApiError>,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    origin_groups: Vec<OriginGroup>,
    total_count: i32,
}

impl EdgeOne {
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

    fn full_domain_name(domain: &Domain) -> String {
        if !domain.sub_domain.is_empty() && domain.sub_domain != "@" {
            format!("{}.{}", domain.sub_domain, domain.domain_name)
        } else {
            domain.domain_name.clone()
        }
    }

    async fn request<T: serde::de::DeserializeOwned>(
        &self,
        action: &str,
        payload: &serde_json::Value,
    ) -> Result<T, String> {
        let signer = ddns_rs_core::signer::TencentSigner::new(&self.dns_conf.DNS.ID, &self.dns_conf.DNS.Secret);
        let (authorization, host, action, timestamp) = signer.sign("teo", action, &payload.to_string());
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

    async fn get_zone(&self, domain: &str) -> Result<ZoneResp, String> {
        let ascii = idna::domain_to_ascii(domain).unwrap_or_else(|_| domain.to_string());
        let payload = serde_json::json!({
            "Filters": [{ "Name": "zone-name", "Values": [ascii] }]
        });
        self.request("DescribeZones", &payload).await
    }

    async fn get_zone_id(&self, domain: &Domain) -> Result<String, String> {
        let params = domain.custom_params();
        if let Some(zone_id) = params.get("ZoneId") {
            if !zone_id.is_empty() {
                return Ok(zone_id.clone());
            }
        }
        let zone_result = self.get_zone(&domain.domain_name).await?;
        if zone_result.response.total_count <= 0 {
            return Err(format!("在 EdgeOne 中未找到站点: {}", domain.domain_name));
        }
        for zone in zone_result.response.zones {
            if zone.zone_name == domain.domain_name {
                return Ok(zone.zone_id);
            }
        }
        Err(format!("在 EdgeOne 中未找到站点: {}", domain.domain_name))
    }

    async fn get_record_list(&self, domain: &Domain, record_type: &str, zone_id: &str) -> Result<RecordResp, String> {
        let name = idna::domain_to_ascii(&Self::full_domain_name(domain))
            .unwrap_or_else(|_| Self::full_domain_name(domain));
        let payload = serde_json::json!({
            "ZoneId": zone_id,
            "Filters": [
                { "Name": "name", "Values": [name] },
                { "Name": "type", "Values": [record_type] }
            ]
        });
        self.request("DescribeDnsRecords", &payload).await
    }

    fn get_location(&self, domain: &Domain) -> String {
        domain
            .custom_params()
            .get("Location")
            .cloned()
            .unwrap_or_else(|| "Default".to_string())
    }

    fn is_origin_group_domain(domain: &Domain) -> bool {
        let params = domain.custom_params();
        params.contains_key("GroupId") || params.contains_key("OriginGroupName")
    }

    async fn add_update(&mut self, record_type: &str, ip_addr: &str, domains: Vec<Domain>) {
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            if Self::is_origin_group_domain(&domain) {
                continue;
            }

            let zone_result = match self.get_zone(&domain.domain_name).await {
                Ok(z) => z,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };
            if zone_result.response.total_count <= 0
                || zone_result.response.zones.is_empty()
                || zone_result.response.zones[0].zone_name != domain.domain_name
            {
                ddns_rs_core::log_msg!("查询域名信息发生异常! %s", "zone not found");
                domain.update_status = UpdateStatus::Failed;
                return;
            }
            let zone_id = zone_result.response.zones[0].zone_id.clone();

            let record_result = match self.get_record_list(&domain, record_type, &zone_id).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            let params = domain.custom_params();
            let record_selected = if let Some(record_id) = params.get("RecordId") {
                record_result
                    .response
                    .dns_records
                    .iter()
                    .find(|r| r.record_id == *record_id)
                    .cloned()
            } else {
                record_result.response.dns_records.iter().find(|r| {
                    r.status == "enable"
                        || (r.status == "disable" && r.content == ip_addr)
                }).cloned()
            };

            if let Some(record) = record_selected {
                self.modify(&record, &mut domain, record_type, ip_addr, &zone_id).await;
            } else {
                self.create(&mut domain, record_type, ip_addr, &zone_id).await;
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str, zone_id: &str) {
        let name = idna::domain_to_ascii(&Self::full_domain_name(domain))
            .unwrap_or_else(|_| Self::full_domain_name(domain));
        let record = serde_json::json!({
            "ZoneId": zone_id,
            "Name": name,
            "Type": record_type,
            "Content": ip_addr,
            "Location": self.get_location(domain),
            "TTL": self.ttl,
        });
        let result: Result<StatusResp, String> = self.request("CreateDnsRecord", &record).await;
        match result {
            Ok(status) => {
                if status.response.error.is_none() {
                    ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    let err = status.response.error.unwrap();
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

    async fn modify(&self, record: &DnsRecord, domain: &mut Domain, record_type: &str, ip_addr: &str, zone_id: &str) {
        if record.content == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }
        let name = idna::domain_to_ascii(&Self::full_domain_name(domain))
            .unwrap_or_else(|_| Self::full_domain_name(domain));
        let payload = serde_json::json!({
            "ZoneId": zone_id,
            "DnsRecords": [{
                "ZoneId": zone_id,
                "Name": name,
                "Type": record_type,
                "Content": ip_addr,
                "Location": self.get_location(domain),
                "TTL": self.ttl,
                "RecordId": record.record_id,
            }]
        });
        let result: Result<StatusResp, String> = self.request("ModifyDnsRecords", &payload).await;
        match result {
            Ok(status) => {
                if status.response.error.is_none() {
                    ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
                    domain.update_status = UpdateStatus::Success;
                } else {
                    let err = status.response.error.unwrap();
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

    // ---------- Origin group handling ----------

    fn get_origin_weight(domain: &Domain) -> i32 {
        let mut weight = 100;
        if let Some(s) = domain.custom_params().get("Weight") {
            if let Ok(v) = s.parse::<i32>() {
                if v > 0 {
                    weight = v;
                }
            }
        }
        weight
    }

    fn has_domain(domains: &[Domain], full_domain: &str) -> bool {
        domains.iter().any(|d| d.display() == full_domain)
    }

    fn get_desired_origin_records(&self, tuple: &DomainTuple) -> Result<Vec<OriginRecord>, String> {
        let domain = &tuple.primary;
        let domain_name = domain.display();
        let weight = Self::get_origin_weight(domain);
        let mut records = Vec::with_capacity(2);

        if Self::has_domain(&self.domains.ipv4_domains, &domain_name) {
            if self.domains.ipv4_addr.is_empty() {
                return Err(format!("未能获取域名 {} 对应的 IPv4 地址", domain_name));
            }
            records.push(OriginRecord {
                record: self.domains.ipv4_addr.clone(),
                record_type: ORIGIN_RECORD_TYPE.to_string(),
                weight,
            });
        }
        if Self::has_domain(&self.domains.ipv6_domains, &domain_name) {
            if self.domains.ipv6_addr.is_empty() {
                return Err(format!("未能获取域名 {} 对应的 IPv6 地址", domain_name));
            }
            records.push(OriginRecord {
                record: self.domains.ipv6_addr.clone(),
                record_type: ORIGIN_RECORD_TYPE.to_string(),
                weight,
            });
        }
        if records.is_empty() {
            return Err(format!("域名 {} 未配置可更新的源站记录", domain_name));
        }
        Ok(records)
    }

    async fn get_origin_group(&self, domain: &Domain, zone_id: &str) -> Result<OriginGroup, String> {
        let params = domain.custom_params();
        let filter = if let Some(group_id) = params.get("GroupId") {
            ("origin-group-id".to_string(), group_id.clone())
        } else if let Some(name) = params.get("OriginGroupName") {
            ("origin-group-name".to_string(), name.clone())
        } else {
            return Err("请在域名后追加 ?GroupId=xxx 或 ?OriginGroupName=xxx".to_string());
        };
        let payload = serde_json::json!({
            "ZoneId": zone_id,
            "Filters": [{ "Name": filter.0, "Values": [filter.1] }]
        });
        let result: OriginGroupResp = self.request("DescribeOriginGroup", &payload).await?;
        if let Some(err) = &result.response.error {
            return Err(err.message.clone());
        }
        if result.response.total_count <= 0 || result.response.origin_groups.is_empty() {
            return Err(format!("在 EdgeOne 中未找到源站组: {}", domain.display()));
        }

        if let Some(group_id) = params.get("GroupId") {
            for group in &result.response.origin_groups {
                if group.group_id == *group_id {
                    return Ok(group.clone());
                }
            }
            return Err(format!("在 EdgeOne 中未找到源站组 GroupId={}", group_id));
        }

        let group_name = params.get("OriginGroupName").cloned().unwrap_or_default();
        for group in &result.response.origin_groups {
            if group.name == group_name {
                return Ok(group.clone());
            }
        }
        if result.response.origin_groups.len() == 1 {
            return Ok(result.response.origin_groups[0].clone());
        }
        Err("找到多个名称匹配的源站组，请改用 GroupId 指定唯一源站组".to_string())
    }

    fn same_origin_records(current: &[OriginRecord], desired: &[OriginRecord]) -> bool {
        if current.len() != desired.len() {
            return false;
        }
        let mut current_keys: Vec<String> = current
            .iter()
            .map(|r| format!("{}|{}|{}", r.record, r.record_type, r.weight))
            .collect();
        let mut desired_keys: Vec<String> = desired
            .iter()
            .map(|r| format!("{}|{}|{}", r.record, r.record_type, r.weight))
            .collect();
        current_keys.sort();
        desired_keys.sort();
        current_keys == desired_keys
    }

    async fn add_update_origin_groups(&mut self, domain_cache: HashMap<String, DomainTuple>) {
        for (_, tuple) in domain_cache {
            if !Self::is_origin_group_domain(&tuple.primary) {
                continue;
            }
            let zone_id = match self.get_zone_id(&tuple.primary).await {
                Ok(z) => z,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询 EdgeOne 站点信息发生异常! %s", e);
                    let mut t = tuple;
                    t.set_update_status(UpdateStatus::Failed);
                    continue;
                }
            };
            let records = match self.get_desired_origin_records(&tuple) {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("整理 EdgeOne 源站组记录失败! %s", e);
                    let mut t = tuple;
                    t.set_update_status(UpdateStatus::Failed);
                    continue;
                }
            };
            let origin_group = match self.get_origin_group(&tuple.primary, &zone_id).await {
                Ok(g) => g,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询 EdgeOne 源站组信息发生异常! %s", e);
                    let mut t = tuple;
                    t.set_update_status(UpdateStatus::Failed);
                    continue;
                }
            };
            self.modify_origin_group(&origin_group, tuple, &zone_id, records).await;
        }
    }

    async fn modify_origin_group(&self, origin_group: &OriginGroup, mut tuple: DomainTuple, zone_id: &str, records: Vec<OriginRecord>) {
        if Self::same_origin_records(&origin_group.records, &records) {
            let mut values: Vec<String> = records.iter().map(|r| r.record.clone()).collect();
            values.sort();
            ddns_rs_core::log_msg!("你的IP %s 没有变化, EdgeOne 源站组 %s", values.join(","), origin_group.name);
            return;
        }

        let payload = serde_json::json!({
            "ZoneId": zone_id,
            "GroupId": origin_group.group_id,
            "Records": records,
        });
        let result: Result<StatusResp, String> = self.request("ModifyOriginGroup", &payload).await;
        match result {
            Ok(status) => {
                if status.response.error.is_none() {
                    let mut values: Vec<String> = records.iter().map(|r| r.record.clone()).collect();
                    values.sort();
                    ddns_rs_core::log_msg!("更新 EdgeOne 源站组 %s 成功! IP: %s", origin_group.name, values.join(","));
                    tuple.set_update_status(UpdateStatus::Success);
                } else {
                    let err = status.response.error.unwrap();
                    ddns_rs_core::log_msg!("更新 EdgeOne 源站组 %s 失败! 异常信息: %s", origin_group.name, err.message);
                    tuple.set_update_status(UpdateStatus::Failed);
                }
            }
            Err(e) => {
                ddns_rs_core::log_msg!("更新 EdgeOne 源站组 %s 失败! 异常信息: %s", origin_group.name, e);
                tuple.set_update_status(UpdateStatus::Failed);
            }
        }
    }

    fn build_domain_tuples(&mut self) -> HashMap<String, DomainTuple> {
        let mut results: HashMap<String, DomainTuple> = HashMap::new();
        let (ipv4_addr, ipv4_domains) = self.domains.get_new_ip_result("A");
        let (ipv6_addr, ipv6_domains) = self.domains.get_new_ip_result("AAAA");

        let mut append = |ip_addr: String, ret_domains: Vec<Domain>, record_type: &str| {
            if ip_addr.is_empty() {
                return;
            }
            for domain in ret_domains {
                let key = domain.display();
                if let Some(tuple) = results.get_mut(&key) {
                    if tuple.record_type != record_type {
                        tuple.record_type = "A/AAAA".to_string();
                    }
                    tuple.primary = domain.clone();
                    tuple.domains.push(domain.clone());
                    tuple.ip_addrs.push(ip_addr.clone());
                } else {
                    let tuple = DomainTuple {
                        record_type: record_type.to_string(),
                        primary: domain.clone(),
                        domains: vec![domain.clone()],
                        ip_addrs: vec![ip_addr.clone()],
                        ipv4_addr: self.domains.ipv4_addr.clone(),
                        ipv6_addr: self.domains.ipv6_addr.clone(),
                    };
                    results.insert(key, tuple);
                }
            }
        };

        append(ipv4_addr, ipv4_domains, "A");
        append(ipv6_addr, ipv6_domains, "AAAA");
        results
    }
}

#[async_trait]
impl crate::engine::DnsProvider for EdgeOne {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        let domain_cache = self.build_domain_tuples();
        self.add_update_origin_groups(domain_cache).await;

        let (ipv4_addr, ipv4_domains) = self.domains.get_new_ip_result("A");
        let (ipv6_addr, ipv6_domains) = self.domains.get_new_ip_result("AAAA");
        self.add_update("A", &ipv4_addr, ipv4_domains).await;
        self.add_update("AAAA", &ipv6_addr, ipv6_domains).await;
        self.domains.clone()
    }
}
