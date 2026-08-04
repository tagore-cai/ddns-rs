use ddns_rs_core::config::{
    check_password, clear_config_cache, reset_password, save_config, Config, CONFIG_FILE_PATH_ENV,
};
use std::path::PathBuf;
use std::sync::Mutex;

/// Serializes tests that mutate the global config cache and the
/// DDNS_CONFIG_FILE_PATH env var (they cannot run concurrently).
static SERIAL: Mutex<()> = Mutex::new(());

struct TempConfig {
    dir: PathBuf,
    path: PathBuf,
}

impl TempConfig {
    fn new(name: &str) -> TempConfig {
        let dir = std::env::temp_dir().join(format!("ddns-rs-test-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested/dir/ddns-rs-config.yaml");
        std::env::set_var(CONFIG_FILE_PATH_ENV, &path);
        clear_config_cache();
        TempConfig { dir, path }
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        std::env::remove_var(CONFIG_FILE_PATH_ENV);
        clear_config_cache();
    }
}

/// save_config must create the parent directory automatically, because on a
/// fresh OpenWrt install /etc/ddns-rs may not exist yet and a bare write
/// would fail, breaking the first-login flow.
#[test]
fn test_save_config_creates_parent_dir() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("save");

    assert!(!tc.path.exists());
    let conf = Config::default();
    save_config(&conf).expect("save_config should create parent dirs and succeed");

    assert!(tc.path.exists(), "config file should exist after save");

    // Verify the saved file parses back.
    let content = std::fs::read_to_string(&tc.path).unwrap();
    let parsed: Config = serde_yaml::from_str(&content).unwrap();
    assert!(parsed.DnsConf.is_empty());
}

/// reset_password on a missing config must create a default config with
/// username=admin and the hashed password, so the LuCI reset button works
/// on a brand-new install where /etc/ddns-rs doesn't exist yet.
#[test]
fn test_reset_password_creates_config_on_missing() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("reset-missing");

    reset_password("admin12345");

    assert!(tc.path.exists(), "reset_password should create the config file");
    let content = std::fs::read_to_string(&tc.path).unwrap();
    let conf: Config = serde_yaml::from_str(&content).unwrap();
    assert_eq!(conf.User.Username, "admin", "username must be reset to admin");
    assert!(
        !conf.User.Password.is_empty() && conf.User.Password.starts_with('$'),
        "password must be bcrypt-hashed, got: {}",
        conf.User.Password
    );
}

/// reset_password on an existing config must overwrite the username to
/// admin and re-hash the password (previously it kept the old username,
/// which broke admin login when the stored username was empty).
#[test]
fn test_reset_password_overwrites_username() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("reset-existing");

    // Existing config with a different username and a weak placeholder.
    let existing = "dnsconf: []\nuser:\n  username: olduser\n  password: oldhash\n";
    std::fs::create_dir_all(tc.path.parent().unwrap()).unwrap();
    std::fs::write(&tc.path, existing).unwrap();
    clear_config_cache();

    reset_password("admin12345");

    let content = std::fs::read_to_string(&tc.path).unwrap();
    let conf: Config = serde_yaml::from_str(&content).unwrap();
    assert_eq!(conf.User.Username, "admin", "username should be reset to admin");
    assert_ne!(conf.User.Password, "oldhash", "password should be re-hashed");
    assert!(conf.User.Password.starts_with('$'));
}

/// reset_password must persist the config to the configured path even when
/// the parent directory does not exist.
#[test]
fn test_reset_password_persists_to_env_path() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("reset-persist");

    reset_password("admin12345");

    assert_eq!(
        std::env::var(CONFIG_FILE_PATH_ENV).unwrap(),
        tc.path.to_string_lossy()
    );
    assert!(tc.path.exists(), "config must be saved to the env path, not the default ~/.ddns_go_config.yaml");
}

/// check_password must accept admin12345 (the default LuCI credentials)
/// and reject obviously weak passwords, regardless of the WAN flag.
#[test]
fn test_check_password_accepts_admin12345() {
    // admin12345 has enough entropy (>= 30 bits) for both thresholds.
    assert!(check_password("admin12345", false).is_ok(), "admin12345 ok at 30 bits");
    assert!(check_password("admin12345", true).is_ok(), "admin12345 ok at 25 bits");

    // Weak passwords must be rejected.
    assert!(check_password("123456", false).is_err(), "123456 too weak");
    assert!(check_password("aaaa", false).is_err(), "aaaa too weak");
    assert!(check_password("admin", false).is_err(), "admin too weak");
}

/// The bcrypt hash used by the LuCI reset flow must verify admin12345.
#[test]
fn test_luci_default_password_hash() {
    let hash = "$2a$10$G1xO1cVUYtSpPYwV/Jk3l.u7PxLUxo03wntWG6VA9BxAftNWfZEhK";
    assert!(
        ddns_rs_core::password::verify("admin12345", hash),
        "LuCI default hash must verify admin12345"
    );
    assert!(
        !ddns_rs_core::password::verify("admin", hash),
        "hash must not verify other passwords"
    );
}
