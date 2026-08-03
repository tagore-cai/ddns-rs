//! DNS provider engine: trait, scheduling, webhook execution, and the
//! endpoint list used to wait for network connectivity.
//!
//! This lives in ddns-providers because it is inherently coupled to the
//! concrete providers (it needs to know their endpoints and construct them).

use ddns_rs_core::config::{DnsConfig, UpdateStatus, Webhook};
use ddns_rs_core::domain::{Domain, Domains};
use ddns_rs_core::ipcache::IpCache;
use async_trait::async_trait;
use std::sync::Mutex;

static UPDATED_FAILED_TIMES: Mutex<u32> = Mutex::new(0);
static GLOBAL_CACHES: Mutex<Vec<(IpCache, IpCache)>> = Mutex::new(Vec::new());

/// Endpoints used by `wait_internet` to determine network readiness.
/// These are the public endpoints of the providers this crate ships.
pub const ADDRESSES: [&str; 16] = [
    "https://alidns.aliyuncs.com/",
    "https://alidns.aliyuncs.com/",
    "https://api.baidu.com",
    "https://api.cloudflare.com/client/v4/zones",
    "https://dnsapi.cn/Record.List",
    "https://dnsapi.cn/Record.List",
    "https://dns.myhuaweicloud.com",
    "https://api.namecheap.com",
    "https://www.namesilo.com/api",
    "https://porkbun.com",
    "https://dnspod.tencentcloudapi.com",
    "https://api.dynadot.com",
    "https://dynv6.com",
    "https://api.gcorelabs.com",
    "https://api.edgeone.tencentcloudapi.com",
    "https://www.cloudns.net",
];

/// DNS provider trait. Implemented by every provider in this crate.
#[async_trait]
pub trait DnsProvider: Send {
    async fn add_update_domain_records(&mut self) -> Domains;
}

/// A factory that constructs a provider by name.
pub type ProviderFactory = fn(&str, &DnsConfig, IpCache, IpCache) -> Box<dyn DnsProvider>;

/// Run once: update all configured DNS providers using the given factory.
pub async fn run_once_with(factory: &ProviderFactory) {
    let conf = match ddns_rs_core::config::get_config_cached() {
        Ok(c) => c,
        Err(_) => return,
    };

    let mut caches: Vec<(IpCache, IpCache)> = {
        let mut global = GLOBAL_CACHES.lock().unwrap();
        if global.len() == conf.DnsConf.len() {
            global.clone()
        } else {
            let mut fresh = Vec::with_capacity(conf.DnsConf.len());
            for _ in &conf.DnsConf {
                fresh.push((IpCache::default(), IpCache::default()));
            }
            *global = fresh.clone();
            fresh
        }
    };

    for (i, dc) in conf.DnsConf.iter().enumerate() {
        let mut provider = factory(&dc.DNS.Name, dc, caches[i].0.clone(), caches[i].1.clone());
        let domains = provider.add_update_domain_records().await;

        // Write back the IP cache state so subsequent runs reuse it.
        caches[i].0 = domains.ipv4_cache.clone();
        caches[i].1 = domains.ipv6_cache.clone();

        // webhook
        let (v4_status, v6_status) = exec_webhook(&domains, &conf.Webhook).await;
        if v4_status == UpdateStatus::Failed {
            caches[i].0 = IpCache::default();
        }
        if v6_status == UpdateStatus::Failed {
            caches[i].1 = IpCache::default();
        }
    }
    *GLOBAL_CACHES.lock().unwrap() = caches;
}

/// Force a re-comparison against DNS providers on the next run.
/// Mirrors Go's util.ForceCompareGlobal behavior after saving config.
pub fn force_compare() {
    *GLOBAL_CACHES.lock().unwrap() = Vec::new();
}

/// Run the update loop every `delay` seconds.
pub async fn run_timer(factory: &'static ProviderFactory, delay: std::time::Duration) {
    loop {
        run_once_with(factory).await;
        tokio::time::sleep(delay).await;
    }
}

/// Determine update status of a list of domains.
fn get_domains_status(domains: &[Domain]) -> UpdateStatus {
    let mut success_num = 0;
    for d in domains {
        match d.update_status {
            UpdateStatus::Failed => return UpdateStatus::Failed,
            UpdateStatus::Success => success_num += 1,
            UpdateStatus::Nothing => {}
        }
    }
    if success_num > 0 {
        return UpdateStatus::Success;
    }
    UpdateStatus::Nothing
}

/// Execute webhook if configured and status changed.
pub async fn exec_webhook(domains: &Domains, webhook: &Webhook) -> (UpdateStatus, UpdateStatus) {
    let v4_status = get_domains_status(&domains.ipv4_domains);
    let v6_status = get_domains_status(&domains.ipv6_domains);

    if webhook.WebhookURL.is_empty()
        || (v4_status == UpdateStatus::Nothing && v6_status == UpdateStatus::Nothing)
    {
        return (v4_status, v6_status);
    }

    // Only trigger webhook on the 3rd consecutive failure
    let (should_trigger, _failed_times) = {
        let mut ft = UPDATED_FAILED_TIMES.lock().unwrap();
        if v4_status == UpdateStatus::Failed || v6_status == UpdateStatus::Failed {
            *ft += 1;
            if *ft != 3 {
                let cur = *ft;
                ddns_rs_core::log_msg!(
                    "将不会触发Webhook, 仅在第 3 次失败时触发一次Webhook, 当前失败次数：%d",
                    cur
                );
                (false, cur)
            } else {
                (true, *ft)
            }
        } else {
            *ft = 0;
            (true, *ft)
        }
    };
    if !should_trigger {
        return (v4_status, v6_status);
    }
    let timestamp = jiff::Timestamp::now().as_second().to_string();
    let request_url = replace_para(domains, &webhook.WebhookURL, v4_status, v6_status, &timestamp);

    let client = ddns_rs_core::httpclient::create_http_client();

    let headers = extract_headers(&webhook.WebhookHeaders);

    let mut builder = if webhook.WebhookRequestBody.is_empty() {
        client.get(request_url)
    } else {
        let body = replace_para(domains, &webhook.WebhookRequestBody, v4_status, v6_status, &timestamp);
        let content_type = if serde_json::from_str::<serde_json::Value>(&body).is_ok() {
            "application/json"
        } else if body.starts_with('{') || body.starts_with('[') {
            ddns_rs_core::log_msg!("Webhook中的 RequestBody JSON 无效");
            "application/json"
        } else {
            "application/x-www-form-urlencoded"
        };
        let mut b = client.post(request_url);
        b = b.header("content-type", content_type);
        b = b.body(body);
        b
    };
    for (k, v) in &headers {
        builder = builder.header(k, v);
    }

    match builder.send().await {
        Ok(resp) => match resp.text().await {
            Ok(text) => ddns_rs_core::log_msg!("Webhook调用成功! 返回数据：%s", text),
            Err(e) => ddns_rs_core::log_msg!("Webhook调用失败! 异常信息：%s", e),
        },
        Err(e) => ddns_rs_core::log_msg!("Webhook调用失败! 异常信息：%s", e),
    }
    (v4_status, v6_status)
}

/// Parse WebhookHeaders text into a map of "key: value" lines.
fn extract_headers(s: &str) -> std::collections::BTreeMap<String, String> {
    let mut headers = std::collections::BTreeMap::new();
    for line in s.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.splitn(2, ':').collect();
        if parts.len() != 2 {
            ddns_rs_core::log_msg!("Webhook Header不正确: %s", line);
            continue;
        }
        headers.insert(parts[0].trim().to_string(), parts[1].trim().to_string());
    }
    headers
}

fn replace_para(
    domains: &Domains,
    org: &str,
    ipv4_result: UpdateStatus,
    ipv6_result: UpdateStatus,
    timestamp: &str,
) -> String {
    let v4_domains = domains
        .ipv4_domains
        .iter()
        .map(|d| d.display())
        .collect::<Vec<_>>()
        .join(",");
    let v6_domains = domains
        .ipv6_domains
        .iter()
        .map(|d| d.display())
        .collect::<Vec<_>>()
        .join(",");
    org.replace("#{ipv4Addr}", &domains.ipv4_addr)
        .replace("#{ipv4Result}", &ddns_rs_core::logger::t(ipv4_result.as_str(), &[]))
        .replace("#{ipv4Domains}", &v4_domains)
        .replace("#{ipv6Addr}", &domains.ipv6_addr)
        .replace("#{ipv6Result}", &ddns_rs_core::logger::t(ipv6_result.as_str(), &[]))
        .replace("#{ipv6Domains}", &v6_domains)
        .replace("#{timestamp}", timestamp)
}

/// Wait until network is reachable.
pub fn wait_internet() {
    ddns_rs_core::netiface::wait_internet(&ADDRESSES);
}
