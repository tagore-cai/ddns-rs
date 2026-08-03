use crate::config::UpdateStatus;
use crate::ipcache::IpCache;
use std::collections::HashMap;

pub mod ipgetter;

/// A parsed domain entity.
#[derive(Debug, Clone, Default)]
pub struct Domain {
    /// Root domain, e.g. example.com
    pub domain_name: String,
    /// Sub domain, e.g. www
    pub sub_domain: String,
    /// Custom query params.
    pub custom_params: String,
    /// Update status.
    pub update_status: UpdateStatus,
}

impl Domain {
    pub fn display(&self) -> String {
        if !self.sub_domain.is_empty() {
            return format!("{}.{}", self.sub_domain, self.domain_name);
        }
        self.domain_name.clone()
    }

    /// Full domain, using `@` for root.
    pub fn full_domain(&self) -> String {
        if !self.sub_domain.is_empty() {
            return format!("{}.{}", self.sub_domain, self.domain_name);
        }
        format!("@.{}", self.domain_name)
    }

    /// Sub domain, empty means `@`.
    pub fn sub_domain(&self) -> String {
        if !self.sub_domain.is_empty() {
            return self.sub_domain.clone();
        }
        "@".to_string()
    }

    /// Parse custom params into a query map.
    pub fn custom_params(&self) -> std::collections::BTreeMap<String, String> {
        let mut map = std::collections::BTreeMap::new();
        if self.custom_params.is_empty() {
            return map;
        }
        for (k, v) in url::form_urlencoded::parse(self.custom_params.as_bytes()) {
            map.insert(k.into_owned(), v.into_owned());
        }
        map
    }

    /// Convert domain to ASCII (Punycode).
    pub fn to_ascii(&self) -> String {
        idna::domain_to_ascii(&self.display()).unwrap_or_else(|_| self.display())
    }
}

/// All domains for IPv4/IPv6.
#[derive(Debug, Clone, Default)]
pub struct Domains {
    pub ipv4_addr: String,
    pub ipv4_cache: IpCache,
    pub ipv4_domains: Vec<Domain>,
    pub ipv6_addr: String,
    pub ipv6_cache: IpCache,
    pub ipv6_domains: Vec<Domain>,
}

/// A tuple of domains for one record type.
#[derive(Debug, Clone)]
pub struct DomainTuple {
    pub record_type: String,
    pub primary: Domain,
    pub domains: Vec<Domain>,
    pub ip_addrs: Vec<String>,
    pub ipv4_addr: String,
    pub ipv6_addr: String,
}

/// key: Domain.display()
pub type DomainTuples = HashMap<String, DomainTuple>;

impl Domains {
    pub async fn get_new_ip(&mut self, dns_conf: &crate::config::DnsConfig) {
        self.ipv4_domains = check_parse_domains(&dns_conf.Ipv4.Domains);
        self.ipv6_domains = check_parse_domains(&dns_conf.Ipv6.Domains);

        // IPv4
        if dns_conf.Ipv4.Enable && !self.ipv4_domains.is_empty() {
            let ipv4 = ipgetter::get_ipv4_addr(dns_conf).await;
            if !ipv4.is_empty() {
                self.ipv4_addr = ipv4;
                self.ipv4_cache.times_failed_ip = 0;
            } else {
                self.ipv4_cache.times_failed_ip += 1;
                if self.ipv4_cache.times_failed_ip == 3 {
                    if let Some(d) = self.ipv4_domains.first_mut() {
                        d.update_status = UpdateStatus::Failed;
                    }
                }
                crate::log_msg!("未能获取IPv4地址, 将不会更新");
            }
        }

        // IPv6
        if dns_conf.Ipv6.Enable && !self.ipv6_domains.is_empty() {
            let ipv6 = ipgetter::get_ipv6_addr(dns_conf).await;
            if !ipv6.is_empty() {
                self.ipv6_addr = ipv6;
                self.ipv6_cache.times_failed_ip = 0;
            } else {
                self.ipv6_cache.times_failed_ip += 1;
                if self.ipv6_cache.times_failed_ip == 3 {
                    if let Some(d) = self.ipv6_domains.first_mut() {
                        d.update_status = UpdateStatus::Failed;
                    }
                }
                crate::log_msg!("未能获取IPv6地址, 将不会更新");
            }
        }
    }

    pub fn get_new_ip_result(&mut self, record_type: &str) -> (String, Vec<Domain>) {
        if record_type == "AAAA" {
            if self.ipv6_cache.check(&self.ipv6_addr) {
                return (self.ipv6_addr.clone(), self.ipv6_domains.clone());
            } else {
                crate::log_msg!(
                    "IPv6未改变, 将等待 %d 次后与DNS服务商进行比对",
                    self.ipv6_cache.times
                );
                return (String::new(), self.ipv6_domains.clone());
            }
        }
        if self.ipv4_cache.check(&self.ipv4_addr) {
            return (self.ipv4_addr.clone(), self.ipv4_domains.clone());
        }
        crate::log_msg!(
            "IPv4未改变, 将等待 %d 次后与DNS服务商进行比对",
            self.ipv4_cache.times
        );
        (String::new(), self.ipv4_domains.clone())
    }

    pub fn get_all_new_ip_result(&mut self, multi_record_type: &str) -> DomainTuples {
        let (ipv4_addr, ipv4_domains) = self.get_new_ip_result("A");
        let (ipv6_addr, ipv6_domains) = self.get_new_ip_result("AAAA");
        if ipv4_addr.is_empty() && ipv6_addr.is_empty() {
            return HashMap::new();
        }

        let mut results: DomainTuples = HashMap::new();
        append_tuples(
            &mut results,
            &ipv4_addr,
            &ipv4_domains,
            multi_record_type,
            &DomainTuple {
                record_type: "A".to_string(),
                primary: Domain::default(),
                domains: vec![],
                ip_addrs: vec![],
                ipv4_addr: self.ipv4_addr.clone(),
                ipv6_addr: self.ipv6_addr.clone(),
            },
        );
        append_tuples(
            &mut results,
            &ipv6_addr,
            &ipv6_domains,
            multi_record_type,
            &DomainTuple {
                record_type: "AAAA".to_string(),
                primary: Domain::default(),
                domains: vec![],
                ip_addrs: vec![],
                ipv4_addr: self.ipv4_addr.clone(),
                ipv6_addr: self.ipv6_addr.clone(),
            },
        );
        results
    }
}

fn append_tuples(
    results: &mut DomainTuples,
    ip_addr: &str,
    ret_domains: &[Domain],
    multi_record_type: &str,
    template: &DomainTuple,
) {
    if ip_addr.is_empty() {
        return;
    }
    for domain in ret_domains {
        let key = domain.display();
        if let Some(tuple) = results.get_mut(&key) {
            if tuple.record_type != template.record_type {
                tuple.record_type = multi_record_type.to_string();
            }
            tuple.primary = domain.clone();
            tuple.domains.push(domain.clone());
            tuple.ip_addrs.push(ip_addr.to_string());
        } else {
            let mut tuple = template.clone();
            tuple.primary = domain.clone();
            tuple.domains = vec![domain.clone()];
            tuple.ip_addrs = vec![ip_addr.to_string()];
            results.insert(key, tuple);
        }
    }
}

impl DomainTuple {
    pub fn set_update_status(&mut self, status: UpdateStatus) {
        if self.primary.update_status == status {
            return;
        }
        for d in self.domains.iter_mut() {
            d.update_status = status;
        }
    }

    pub fn get_ip_addr_pool(&self, separator: &str) -> String {
        let pool = self
            .primary
            .custom_params()
            .get("IpAddrPool")
            .cloned();
        if let Some(s) = pool {
            if !s.is_empty() {
                return s
                    .replace("{ipv4Addr}", &self.ipv4_addr)
                    .replace("{ipv6Addr}", &self.ipv6_addr);
            }
        }
        match self.record_type.as_str() {
            "A" => self.ipv4_addr.clone(),
            "AAAA" => self.ipv6_addr.clone(),
            _ => format!("{}{}{}", self.ipv4_addr, separator, self.ipv6_addr),
        }
    }
}

/// Parse and validate user-entered domains.
pub fn check_parse_domains(domain_arr: &[String]) -> Vec<Domain> {
    let mut domains = Vec::new();
    for raw in domain_arr {
        let domain_str = raw.trim();
        if domain_str.is_empty() {
            continue;
        }
        let mut domain = Domain::default();

        // Extract custom params: baidu.com?q=1 => [baidu.com, q=1]
        let qp: Vec<&str> = domain_str.splitn(2, '?').collect();
        let domain_part = qp[0];

        // Split sub domain:root domain: www:example.cn.eu.org => [www, example.cn.eu.org]
        let dp: Vec<&str> = domain_part.splitn(2, ':').collect();
        match dp.len() {
            1 => {
                // auto detect
                match psl::domain_str(domain_part) {
                    Some(domain_name) => {
                        domain.domain_name = domain_name.to_string();
                        let domain_name_len = domain_name.len();
                        if domain_part.len() > domain_name_len + 1 {
                            let domain_len = domain_part.len() - domain_name_len - 1;
                            domain.sub_domain = domain_part[..domain_len].to_string();
                        }
                    }
                    None => {
                        crate::log_msg!("域名: %s 不正确", domain_part);
                        crate::log_msg!("异常信息: %s", "invalid domain");
                        continue;
                    }
                }
            }
            2 => {
                let sp: Vec<&str> = dp[1].split('.').collect();
                if sp.len() <= 1 {
                    crate::log_msg!("域名: %s 不正确", domain_part);
                    continue;
                }
                domain.domain_name = dp[1].to_string();
                domain.sub_domain = dp[0].to_string();
            }
            _ => {
                crate::log_msg!("域名: %s 不正确", domain_part);
                continue;
            }
        }

        if qp.len() == 2 {
            if let Ok(u) = url::Url::parse(&format!("https://baidu.com?{}", qp[1])) {
                domain.custom_params = u.query().unwrap_or("").to_string();
            } else {
                crate::log_msg!("域名: %s 解析失败", domain_part);
                continue;
            }
        }
        domains.push(domain);
    }
    domains
}
