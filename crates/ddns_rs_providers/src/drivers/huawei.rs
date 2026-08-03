use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use ddns_rs_core::signer::HuaweiSigner;
use async_trait::async_trait;

const ENDPOINT: &str = "https://dns.myhuaweicloud.com";

pub struct Huaweicloud {
    dns_conf: DnsConfig,
    domains: Domains,
    ttl: i32,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct Recordsets {
    id: String,
    name: String,
    zone_id: String,
    status: Option<String>,
    #[serde(rename = "type")]
    record_type: String,
    ttl: i32,
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]
    records: Vec<String>,
}

#[derive(serde::Deserialize, Debug)]
struct ZonesResp {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    zones: Vec<Zone>,
}

#[derive(serde::Deserialize, Debug)]
struct Zone {
    id: String,
    name: String,
}

#[derive(serde::Deserialize, Debug)]
struct RecordsResp {
    #[serde(default, deserialize_with = "ddns_rs_core::serde_util::deserialize_null_default_vec")]

    recordsets: Vec<Recordsets>,
}

impl Huaweicloud {
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
        method: &str,
        path: &str,
        query: &[(String, String)],
        body: Option<&serde_json::Value>,
    ) -> Result<T, String> {
        let body_bytes = body.map(|b| b.to_string().into_bytes()).unwrap_or_default();
        let signer = HuaweiSigner::new(&self.dns_conf.DNS.ID, &self.dns_conf.DNS.Secret);
        // Go's Signer does not include host in signed headers (host is not in r.Header).
        let (authorization, date, body_hash) = signer.sign(method, path, query, &[], &body_bytes);

        let mut builder = self.http_client.request(
            reqwest::Method::from_bytes(method.as_bytes()).unwrap(),
            format!("{}{}", ENDPOINT, path),
        );
        if !query.is_empty() {
            let qs: Vec<String> = query
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect();
            builder = builder.query(&qs.join("&"));
        }
        builder = builder
            .header("Authorization", authorization)
            .header("X-Sdk-Date", date)
            .header("X-Sdk-Content-Sha256", body_hash)
            .header("content-type", "application/json");
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

    async fn get_zones(&self, domain: &Domain) -> Result<ZonesResp, String> {
        self.request("GET", "/v2/zones", &[("name".to_string(), domain.domain_name.clone())], None).await
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            let custom_params = domain.custom_params();
            let mut query = vec![
                ("name".to_string(), domain.display()),
                ("type".to_string(), record_type.to_string()),
            ];

            if custom_params.contains_key("zone_id") && custom_params.contains_key("recordset_id") {
                let path = format!(
                    "/v2.1/zones/{}/recordsets/{}",
                    custom_params.get("zone_id").unwrap(),
                    custom_params.get("recordset_id").unwrap()
                );
                let record: Recordsets = match self.request("GET", &path, &query, None).await {
                    Ok(r) => r,
                    Err(e) => {
                        ddns_rs_core::log_msg!("查询域名信息发生异常！ %s", e);
                        domain.update_status = UpdateStatus::Failed;
                        return;
                    }
                };
                self.modify(record, &mut domain, &ip_addr).await;
            } else {
                // Copy all custom params
                for (k, v) in &custom_params {
                    query.push((k.clone(), v.clone()));
                }
                // Fix param name
                for (k, _) in query.iter_mut() {
                    if k == "recordset_id" {
                        *k = "id".to_string();
                    }
                }

                let records: RecordsResp = match self.request("GET", "/v2.1/recordsets", &query, None).await {
                    Ok(r) => r,
                    Err(e) => {
                        ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                        domain.update_status = UpdateStatus::Failed;
                        return;
                    }
                };

                let mut find = false;
                for record in &records.recordsets {
                    if record.name == format!("{}.", domain.display()) {
                        self.modify(record.clone(), &mut domain, &ip_addr).await;
                        find = true;
                        break;
                    }
                }

                if !find {
                    let th_id_param_name = if custom_params.contains_key("id") {
                        "id"
                    } else if custom_params.contains_key("recordset_id") {
                        "recordset_id"
                    } else {
                        ""
                    };
                    if !th_id_param_name.is_empty() {
                        ddns_rs_core::log_msg!(
                            "域名 %s 解析未找到，且因添加了参数 %s=%s 导致无法创建。本次更新已被忽略",
                            domain.display(),
                            th_id_param_name,
                            custom_params.get(th_id_param_name).cloned().unwrap_or_default()
                        );
                    } else {
                        self.create(&mut domain, record_type, &ip_addr).await;
                    }
                }
            }
        }
    }

    async fn create(&self, domain: &mut Domain, record_type: &str, ip_addr: &str) {
        let zone = match self.get_zones(domain).await {
            Ok(z) => z,
            Err(e) => {
                ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
        };
        if zone.zones.is_empty() {
            ddns_rs_core::log_msg!("在DNS服务商中未找到根域名: %s", domain.domain_name);
            domain.update_status = UpdateStatus::Failed;
            return;
        }

        let mut zone_id = zone.zones[0].id.clone();
        for z in &zone.zones {
            if z.name == format!("{}.", domain.domain_name) {
                zone_id = z.id.clone();
                break;
            }
        }

        let record = serde_json::json!({
            "type": record_type,
            "name": format!("{}.", domain.display()),
            "records": [ip_addr],
            "ttl": self.ttl,
            "weight": 1,
        });
        let path = format!("/v2.1/zones/{}/recordsets", zone_id);
        let result: Recordsets = match self.request("POST", &path, &[], Some(&record)).await {
            Ok(r) => r,
            Err(e) => {
                ddns_rs_core::log_msg!("新增域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
        };

        if !result.records.is_empty() && result.records[0] == ip_addr {
            ddns_rs_core::log_msg!("新增域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
            domain.update_status = UpdateStatus::Success;
        } else {
            ddns_rs_core::log_msg!(
                "新增域名解析 %s 失败! 异常信息: %s",
                domain.display(),
                result.status.unwrap_or_default()
            );
            domain.update_status = UpdateStatus::Failed;
        }
    }

    async fn modify(&self, record: Recordsets, domain: &mut Domain, ip_addr: &str) {
        if !record.records.is_empty() && record.records[0] == ip_addr {
            ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
            return;
        }

        let body = serde_json::json!({
            "name": record.name,
            "type": record.record_type,
            "records": [ip_addr],
            "ttl": self.ttl,
        });
        let path = format!("/v2.1/zones/{}/recordsets/{}", record.zone_id, record.id);
        let result: Recordsets = match self.request("PUT", &path, &[], Some(&body)).await {
            Ok(r) => r,
            Err(e) => {
                ddns_rs_core::log_msg!("更新域名解析 %s 失败! 异常信息: %s", domain.display(), e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
        };

        if !result.records.is_empty() && result.records[0] == ip_addr {
            ddns_rs_core::log_msg!("更新域名解析 %s 成功! IP: %s", domain.display(), ip_addr);
            domain.update_status = UpdateStatus::Success;
        } else {
            ddns_rs_core::log_msg!(
                "更新域名解析 %s 失败! 异常信息: %s",
                domain.display(),
                result.status.unwrap_or_default()
            );
            domain.update_status = UpdateStatus::Failed;
        }
    }
}

#[async_trait]
impl crate::engine::DnsProvider for Huaweicloud {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
