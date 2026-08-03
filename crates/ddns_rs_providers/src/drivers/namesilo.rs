use ddns_rs_core::config::{DnsConfig, UpdateStatus};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;

const LIST_ENDPOINT: &str = "https://www.namesilo.com/api/dnsListRecords?version=1&type=xml&key=#{password}&domain=#{domain}";
const ADD_ENDPOINT: &str = "https://www.namesilo.com/api/dnsAddRecord?version=1&type=xml&key=#{password}&domain=#{domain}&rrhost=#{host}&rrtype=#{recordType}&rrvalue=#{ip}&rrttl=3600";
const UPDATE_ENDPOINT: &str = "https://www.namesilo.com/api/dnsUpdateRecord?version=1&type=xml&key=#{password}&domain=#{domain}&rrhost=#{host}&rrid=#{recordID}&rrvalue=#{ip}&rrttl=3600";

pub struct NameSilo {
    dns_conf: DnsConfig,
    domains: Domains,
    http_client: reqwest::Client,
}

#[derive(serde::Deserialize, Debug)]
struct Reply {
    code: Option<i32>,
    detail: Option<String>,
    record_id: Option<String>,
    resource_record: Option<Vec<ResourceRecord>>,
}

#[derive(serde::Deserialize, Debug, Clone)]
struct ResourceRecord {
    record_id: String,
    #[serde(rename = "type")]
    record_type: String,
    host: String,
    value: String,
    ttl: Option<i32>,
}

impl NameSilo {
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

    async fn request(&self, url: &str) -> Result<String, String> {
        let resp = self
            .http_client
            .get(url)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        resp.text().await.map_err(|e| e.to_string())
    }

    fn build_url(&self, template: &str, host: &str, domain: &str, record_id: &str, record_type: &str, ip: &str) -> String {
        template
            .replace("#{host}", host)
            .replace("#{domain}", domain)
            .replace("#{password}", &self.dns_conf.DNS.Secret)
            .replace("#{recordID}", record_id)
            .replace("#{recordType}", record_type)
            .replace("#{ip}", ip)
    }

    async fn list_records(&self, domain: &Domain) -> Result<Vec<ResourceRecord>, String> {
        let url = self.build_url(LIST_ENDPOINT, "", &domain.domain_name, "", "", "");
        let body = self.request(&url).await?;
        let doc: roxmltree::Document = roxmltree::Document::parse(&body).map_err(|e| e.to_string())?;
        parse_reply(&doc)
    }

    async fn add_update(&mut self, record_type: &str) {
        let (ip_addr, domains) = self.domains.get_new_ip_result(record_type);
        if ip_addr.is_empty() {
            return;
        }

        for mut domain in domains {
            if domain.sub_domain.is_empty() {
                domain.sub_domain = "@".to_string();
            }

            let items = match self.list_records(&domain).await {
                Ok(r) => r,
                Err(e) => {
                    ddns_rs_core::log_msg!("查询域名信息发生异常! %s", e);
                    domain.update_status = UpdateStatus::Failed;
                    return;
                }
            };

            let record = items.iter().find(|r| {
                r.host == domain.sub_domain && r.record_type == record_type
            }).cloned();

            let is_add = record.is_none();
            let record_id = record.as_ref().map(|r| r.record_id.clone()).unwrap_or_default();
            if let Some(rec) = &record {
                if rec.value == ip_addr {
                    ddns_rs_core::log_msg!("你的IP %s 没有变化, 域名 %s", ip_addr, domain.display());
                    continue;
                }
            }
            self.modify(&mut domain, &record_id, record_type, &ip_addr, is_add).await;
        }
    }

    async fn modify(&self, domain: &mut Domain, record_id: &str, record_type: &str, ip_addr: &str, is_add: bool) {
        let request_type = if is_add { "新增" } else { "更新" };
        let template = if is_add { ADD_ENDPOINT } else { UPDATE_ENDPOINT };
        let url = self.build_url(
            template,
            &domain.sub_domain,
            &domain.domain_name,
            record_id,
            record_type,
            ip_addr,
        );
        let result = match self.request(&url).await {
            Ok(r) => r,
            Err(e) => {
                ddns_rs_core::log_msg!("异常信息: %s", e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
        };
        let doc = match roxmltree::Document::parse(&result) {
            Ok(d) => d,
            Err(e) => {
                ddns_rs_core::log_msg!("异常信息: %s", e);
                domain.update_status = UpdateStatus::Failed;
                return;
            }
        };
        // Extract code/detail from the document
        let code: Option<i32> = doc
            .descendants()
            .find(|n| n.has_tag_name("code"))
            .and_then(|n| n.text())
            .and_then(|t| t.trim().parse().ok());
        let detail: Option<String> = doc
            .descendants()
            .find(|n| n.has_tag_name("detail"))
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string());

        if code == Some(300) {
            ddns_rs_core::log_msg!("{}域名解析 %s 成功! IP: %s", request_type, domain.display(), ip_addr);
            domain.update_status = UpdateStatus::Success;
        } else {
            ddns_rs_core::log_msg!(
                "{}域名解析 %s 失败! 异常信息: %s",
                request_type,
                domain.display(),
                detail.unwrap_or_default()
            );
            domain.update_status = UpdateStatus::Failed;
        }
    }
}

fn parse_reply(doc: &roxmltree::Document) -> Result<Vec<ResourceRecord>, String> {
    let mut records = Vec::new();
    for node in doc.descendants().filter(|n| n.has_tag_name("resource_record")) {
        let mut record = ResourceRecord {
            record_id: String::new(),
            record_type: String::new(),
            host: String::new(),
            value: String::new(),
            ttl: None,
        };
        for child in node.children().filter(|c| c.is_element()) {
            let text = child.text().unwrap_or("").trim().to_string();
            match child.tag_name().name() {
                "record_id" => record.record_id = text,
                "type" => record.record_type = text,
                "host" => record.host = text,
                "value" => record.value = text,
                "ttl" => record.ttl = text.parse().ok(),
                _ => {}
            }
        }
        records.push(record);
    }
    Ok(records)
}

#[async_trait]
impl crate::engine::DnsProvider for NameSilo {
    async fn add_update_domain_records(&mut self) -> Domains {
        self.domains.get_new_ip(&self.dns_conf).await;
        self.add_update("A").await;
        self.add_update("AAAA").await;
        self.domains.clone()
    }
}
