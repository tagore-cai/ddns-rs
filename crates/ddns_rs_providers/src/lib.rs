#![allow(dead_code)]

pub mod callback;
pub mod engine;
pub mod drivers;

use ddns_rs_core::config::DnsConfig;
use crate::engine::DnsProvider;
use ddns_rs_core::ipcache::IpCache;

/// Construct a DNS provider by name. This is the dispatch table that knows
/// about all concrete provider types; ddns-core uses it via its factory hook.
pub fn new_provider(
    name: &str,
    dns_conf: &DnsConfig,
    ipv4_cache: IpCache,
    ipv6_cache: IpCache,
) -> Box<dyn DnsProvider> {
    use drivers::*;
    match name {
        "alidns" => Box::new(alidns::Alidns::new(dns_conf, ipv4_cache, ipv6_cache)),
        "aliesa" => Box::new(aliesa::Aliesa::new(dns_conf, ipv4_cache, ipv6_cache)),
        "dnspod" => Box::new(dnspod::Dnspod::new(dns_conf, ipv4_cache, ipv6_cache)),
        "tencentcloud" => Box::new(tencent::TencentCloud::new(dns_conf, ipv4_cache, ipv6_cache)),
        "edgeone" => Box::new(edgeone::EdgeOne::new(dns_conf, ipv4_cache, ipv6_cache)),
        "trafficroute" => Box::new(traffic_route::TrafficRoute::new(dns_conf, ipv4_cache, ipv6_cache)),
        "cloudflare" => Box::new(cloudflare::Cloudflare::new(dns_conf, ipv4_cache, ipv6_cache)),
        "godaddy" => Box::new(godaddy::GoDaddy::new(dns_conf, ipv4_cache, ipv6_cache)),
        "namecheap" => Box::new(namecheap::NameCheap::new(dns_conf, ipv4_cache, ipv6_cache)),
        "porkbun" => Box::new(porkbun::Porkbun::new(dns_conf, ipv4_cache, ipv6_cache)),
        "vercel" => Box::new(vercel::Vercel::new(dns_conf, ipv4_cache, ipv6_cache)),
        "namesilo" => Box::new(namesilo::NameSilo::new(dns_conf, ipv4_cache, ipv6_cache)),
        "dynv6" => Box::new(dynv6::Dynv6::new(dns_conf, ipv4_cache, ipv6_cache)),
        "gcore" => Box::new(gcore::Gcore::new(dns_conf, ipv4_cache, ipv6_cache)),
        "nsone" => Box::new(nsone::NSOne::new(dns_conf, ipv4_cache, ipv6_cache)),
        "name_com" => Box::new(name_com::NameCom::new(dns_conf, ipv4_cache, ipv6_cache)),
        "spaceship" => Box::new(spaceship::Spaceship::new(dns_conf, ipv4_cache, ipv6_cache)),
        "dynadot" => Box::new(dynadot::Dynadot::new(dns_conf, ipv4_cache, ipv6_cache)),
        "cloudns" => Box::new(cloudns::ClouDNS::new(dns_conf, ipv4_cache, ipv6_cache)),
        "huaweicloud" => Box::new(huawei::Huaweicloud::new(dns_conf, ipv4_cache, ipv6_cache)),
        "baiducloud" => Box::new(baidu::BaiduCloud::new(dns_conf, ipv4_cache, ipv6_cache)),
        "dnsla" => Box::new(dnsla::Dnsla::new(dns_conf, ipv4_cache, ipv6_cache)),
        "nowcn" => Box::new(nowcn::Nowcn::new(dns_conf, ipv4_cache, ipv6_cache)),
        "eranet" => Box::new(eranet::Eranet::new(dns_conf, ipv4_cache, ipv6_cache)),
        "tnethk" => Box::new(tnethk::Tnethk::new(dns_conf, ipv4_cache, ipv6_cache)),
        "rainyun" => Box::new(rainyun::Rainyun::new(dns_conf, ipv4_cache, ipv6_cache)),
        "hipmdnsmgr" => Box::new(hipmdnsmgr::HiPMDnsMgr::new(dns_conf, ipv4_cache, ipv6_cache)),
        "callback" => Box::new(callback::Callback::new(dns_conf, ipv4_cache, ipv6_cache)),
        _ => Box::new(alidns::Alidns::new(dns_conf, ipv4_cache, ipv6_cache)),
    }
}

/// A static factory hook usable by engine::run_once_with / engine::run_timer.
pub static PROVIDER_FACTORY: crate::engine::ProviderFactory = new_provider;
