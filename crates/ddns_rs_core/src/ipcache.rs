/// Cache of last obtained IP addresses.
#[derive(Debug, Clone, Default)]
pub struct IpCache {
    /// Cached address.
    pub addr: String,
    /// Remaining times.
    pub times: i32,
    /// Number of times failed to obtain IP.
    pub times_failed_ip: i32,
}

impl IpCache {
    /// Check whether to compare against DNS provider.
    pub fn check(&mut self, new_addr: &str) -> bool {
        if new_addr.is_empty() {
            return true;
        }
        if self.addr != new_addr || self.times <= 1 {
            let cache_times = std::env::var(crate::config::IP_CACHE_TIMES_ENV)
                .ok()
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(5);
            self.addr = new_addr.to_string();
            self.times = cache_times + 1;
            return true;
        }
        self.addr = new_addr.to_string();
        self.times -= 1;
        false
    }
}
