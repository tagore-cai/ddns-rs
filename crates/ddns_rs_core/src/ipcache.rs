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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::IP_CACHE_TIMES_ENV;
    use std::sync::Mutex;

    static SERIAL: Mutex<()> = Mutex::new(());

    fn clear_env() {
        std::env::remove_var(IP_CACHE_TIMES_ENV);
    }

    #[test]
    fn test_check_empty_addr_returns_true() {
        let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut cache = IpCache::default();
        assert!(cache.check(""), "empty addr must trigger comparison");
    }

    #[test]
    fn test_check_first_ip_triggers() {
        let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_env();
        let mut cache = IpCache::default();
        assert!(cache.check("1.2.3.4"), "first address must trigger");
        assert_eq!(cache.addr, "1.2.3.4");
        assert_eq!(cache.times, 6, "default cache_times=5 -> times=6");
    }

    #[test]
    fn test_check_same_ip_caches_until_times_1() {
        let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_env();
        let mut cache = IpCache::default();
        cache.check("1.2.3.4"); // triggers, times=6
        assert!(!cache.check("1.2.3.4"), "same ip should not trigger");
        assert_eq!(cache.times, 5);
        // Keep feeding the same IP until times hits 1.
        for _ in 0..4 {
            cache.check("1.2.3.4");
        }
        assert_eq!(cache.times, 1);
        assert!(cache.check("1.2.3.4"), "times<=1 must trigger");
    }

    #[test]
    fn test_check_changed_ip_triggers_immediately() {
        let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_env();
        let mut cache = IpCache::default();
        cache.check("1.2.3.4");
        assert!(
            cache.check("5.6.7.8"),
            "a different address must trigger immediately"
        );
    }

    #[test]
    fn test_check_honors_env_cache_times() {
        let _g = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        clear_env();
        std::env::set_var(IP_CACHE_TIMES_ENV, "3");
        let mut cache = IpCache::default();
        assert!(cache.check("1.2.3.4"));
        assert_eq!(cache.times, 4, "cache_times=3 -> times=4");
        assert!(!cache.check("1.2.3.4"));
        clear_env();
    }
}
