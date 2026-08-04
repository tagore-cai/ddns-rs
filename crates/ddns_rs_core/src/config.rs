#![allow(non_snake_case)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;

/// Environment variable for config file path.
pub const CONFIG_FILE_PATH_ENV: &str = "DDNS_CONFIG_FILE_PATH";
pub const IP_CACHE_TIMES_ENV: &str = "DDNS_IP_CACHE_TIMES";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Ipv4Conf {
    #[serde(rename = "enable", default)]
    pub Enable: bool,
    /// url / netInterface / cmd
    #[serde(rename = "gettype", default)]
    pub GetType: String,
    #[serde(rename = "url", default)]
    pub URL: String,
    #[serde(rename = "netinterface", default)]
    pub NetInterface: String,
    #[serde(rename = "cmd", default)]
    pub Cmd: String,
    #[serde(rename = "domains", default)]
    pub Domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Ipv6Conf {
    #[serde(rename = "enable", default)]
    pub Enable: bool,
    #[serde(rename = "gettype", default)]
    pub GetType: String,
    #[serde(rename = "url", default)]
    pub URL: String,
    #[serde(rename = "netinterface", default)]
    pub NetInterface: String,
    #[serde(rename = "cmd", default)]
    pub Cmd: String,
    #[serde(rename = "ipv6reg", default)]
    pub Ipv6Reg: String,
    #[serde(rename = "domains", default)]
    pub Domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DNSConf {
    /// Provider name, e.g. alidns, webhook, cloudflare...
    #[serde(rename = "name", default)]
    pub Name: String,
    #[serde(rename = "id", default)]
    pub ID: String,
    #[serde(rename = "secret", default)]
    pub Secret: String,
    #[serde(rename = "extparam", default)]
    pub ExtParam: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct DnsConfig {
    #[serde(rename = "name", default)]
    pub Name: String,
    #[serde(rename = "ipv4", default)]
    pub Ipv4: Ipv4Conf,
    #[serde(rename = "ipv6", default)]
    pub Ipv6: Ipv6Conf,
    #[serde(rename = "dns", default)]
    pub DNS: DNSConf,
    #[serde(rename = "ttl", default)]
    pub TTL: String,
    #[serde(rename = "httpinterface", default)]
    pub HttpInterface: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct User {
    #[serde(rename = "username", default)]
    pub Username: String,
    #[serde(rename = "password", default)]
    pub Password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Webhook {
    #[serde(rename = "webhookurl", default)]
    pub WebhookURL: String,
    #[serde(rename = "webhookrequestbody", default)]
    pub WebhookRequestBody: String,
    #[serde(rename = "webhookheaders", default)]
    pub WebhookHeaders: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Config {
    #[serde(rename = "dnsconf", default)]
    pub DnsConf: Vec<DnsConfig>,
    #[serde(rename = "user", default)]
    pub User: User,
    #[serde(rename = "webhook", default)]
    pub Webhook: Webhook,
    #[serde(rename = "notallowwanaccess", default)]
    pub NotAllowWanAccess: bool,
    #[serde(rename = "lang", default)]
    pub Lang: String,
}

/// Update status type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStatus {
    /// Not changed
    Nothing,
    /// Update failed
    Failed,
    /// Update success
    Success,
}

impl UpdateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            UpdateStatus::Nothing => "未改变",
            UpdateStatus::Failed => "失败",
            UpdateStatus::Success => "成功",
        }
    }
}

impl Default for UpdateStatus {
    fn default() -> Self {
        UpdateStatus::Nothing
    }
}

// ---------- config cache ----------

static CACHE: Mutex<Option<Config>> = Mutex::new(None);

/// Get the default config file path: ~/.ddns_go_config.yaml
pub fn get_config_file_path_default() -> PathBuf {
    match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
        Ok(home) => PathBuf::from(home).join(".ddns_go_config.yaml"),
        Err(_) => PathBuf::from("../.ddns_go_config.yaml"),
    }
}

/// Get config file path honoring the env var.
pub fn get_config_file_path() -> PathBuf {
    if let Ok(p) = std::env::var(CONFIG_FILE_PATH_ENV) {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    get_config_file_path_default()
}

/// Load config, with caching.
pub fn get_config_cached() -> Result<Config, String> {
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(c) = cache.as_ref() {
        return Ok(c.clone());
    }

    let path = get_config_file_path();
    if !path.exists() {
        return Err(format!("config file not found: {}", path.display()));
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut conf: Config = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;

    // No login info: forbid WAN access.
    if conf.User.Username.is_empty() && conf.User.Password.is_empty() {
        conf.NotAllowWanAccess = true;
    }
    *cache = Some(conf.clone());
    Ok(conf)
}

/// Compatibility with older config files (mirrors Go's Config.CompatibleConfig).
/// - Re-hashes plaintext passwords with bcrypt.
/// - Migrates pre-v5 single DnsConfig to the DnsConf array format.
pub fn compatible_config(conf: &mut Config) {
    // If the password is not bcrypt-hashed, hash and save it.
    if !conf.User.Password.is_empty() && !crate::password::is_hashed_password(&conf.User.Password) {
        if let Ok(hashed) = crate::password::hash(&conf.User.Password) {
            conf.User.Password = hashed;
            let _ = save_config(conf);
        }
    }

    // Migrate pre-v5 config: single DnsConfig in the same file.
    if !conf.DnsConf.is_empty() {
        return;
    }
    let path = get_config_file_path();
    if !path.exists() {
        return;
    }
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return,
    };
    let dns_conf: DnsConfig = match serde_yaml::from_str(&content) {
        Ok(d) => d,
        Err(_) => return,
    };
    if !dns_conf.DNS.Name.is_empty() {
        conf.DnsConf.push(dns_conf);
        update_cache(conf);
    }
}

/// Save config to file, clearing the cache.
pub fn save_config(conf: &Config) -> Result<(), String> {
    let content = serde_yaml::to_string(conf).map_err(|e| e.to_string())?;
    let path = get_config_file_path();
    // Create the parent directory so the config can be saved even on a
    // fresh install where /etc/ddns-rs does not exist yet.
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    crate::log_msg!("配置文件已保存在: %s", path.display());

    *CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    Ok(())
}

/// Update the cached config (used after save).
pub fn update_cache(conf: &Config) {
    *CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(conf.clone());
}

/// Clear the in-memory config cache. Primarily useful in tests where the
/// config path env changes between test cases; the process normally uses a
/// single config path so the cache never needs manual clearing.
pub fn clear_config_cache() {
    *CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

/// Reset the username and password in the config file.
///
/// Resets both the username (to "admin") and the password so that the
/// default web interface login (admin/admin12345 shown by the LuCI plugin)
/// always works, including on fresh configs where the file is missing or
/// the User section is empty.
pub fn reset_password(new_password: &str) {
    let mut conf = match get_config_cached() {
        Ok(c) => c,
        Err(_) => {
            // Config file missing: create a default one so the reset works
            // even on a brand-new install.
            crate::log_msg!(
                "配置文件不存在, 创建默认配置并重置账号密码: %s",
                get_config_file_path().display()
            );
            Config::default()
        }
    };
    crate::logger::init_lang(&conf.Lang);

    match check_password(new_password, conf.NotAllowWanAccess) {
        Ok(hashed) => {
            conf.User.Username = "admin".to_string();
            conf.User.Password = hashed;
            if let Some(p) = get_config_file_path().parent() {
                let _ = std::fs::create_dir_all(p);
            }
            match save_config(&conf) {
                Ok(_) => crate::log_msg!(
                    "用户名 %s 的密码已重置成功! 请重启ddns-go",
                    conf.User.Username
                ),
                Err(e) => crate::log_msg!("异常信息: %s", e),
            }
        }
        Err(e) => crate::log_msg!("{}", e),
    }
}

/// Validate and hash a password.
/// Uses the same entropy algorithm as Go's go-password-validator.
pub fn check_password(new_password: &str, not_allow_wan: bool) -> Result<String, String> {
    let min_entropy_bits: f64 = if not_allow_wan { 25.0 } else { 30.0 };
    if !crate::password_entropy::validate(new_password, min_entropy_bits) {
        return Err(crate::logger::t("密码不安全！尝试使用更复杂的密码", &[]));
    }
    crate::password::hash(new_password)
}
