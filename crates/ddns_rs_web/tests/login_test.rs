use axum::body::to_bytes;
use axum::Json;
use ddns_rs_core::config::{clear_config_cache, save_config, Config, CONFIG_FILE_PATH_ENV};
use ddns_rs_web::dto::LoginData;
use ddns_rs_web::handlers::login_func;
use ddns_rs_web::state::new_state;
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
        let dir = std::env::temp_dir().join(format!("ddns-rs-weblogin-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("ddns-rs-config.yaml");
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

async fn call_login(state: &ddns_rs_web::state::SharedState, user: &str, pass: &str) -> serde_json::Value {
    let data = Json(LoginData {
        username: user.to_string(),
        password: pass.to_string(),
    });
    let resp = login_func(state.clone(), data).await;
    let status = resp.status();
    let body = to_bytes(resp.into_body(), 4096).await.unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::json!({ "raw": String::from_utf8_lossy(&body).to_string() }));
    json["status"] = serde_json::json!(status.as_u16());
    json
}

/// First login with an empty/missing config initializes admin/admin12345
/// and succeeds (this is the flow that previously failed because the
/// parent dir was missing).
#[tokio::test]
async fn test_first_login_initializes_account() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("first");
    let state = new_state();

    let json = call_login(&state, "admin", "admin12345").await;
    assert_eq!(json["status"], 200, "first login should succeed: {}", json);
    assert_eq!(json["Code"], 200, "expected ok code, got: {}", json);

    // Config file must have been written with the credentials.
    let content = std::fs::read_to_string(&tc.path).unwrap();
    let conf: Config = serde_yaml::from_str(&content).unwrap();
    assert_eq!(conf.User.Username, "admin");
}

/// After the account is initialized, correct credentials log in and wrong
/// ones are rejected.
#[tokio::test]
async fn test_login_after_init() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("after");
    let state = new_state();

    // Pre-populate config with admin/admin12345.
    let mut conf = Config::default();
    conf.User.Username = "admin".to_string();
    conf.User.Password = ddns_rs_core::password::hash("admin12345").unwrap();
    save_config(&conf).unwrap();

    // Correct password -> Code 200.
    let json = call_login(&state, "admin", "admin12345").await;
    assert_eq!(json["Code"], 200, "correct login should succeed: {}", json);

    // Wrong password -> Code 500.
    let json = call_login(&state, "admin", "wrongpass").await;
    assert_eq!(json["Code"], 500, "wrong password must fail: {}", json);

    // Wrong username -> Code 500.
    let json = call_login(&state, "nobody", "admin12345").await;
    assert_eq!(json["Code"], 500, "wrong username must fail: {}", json);
}

/// A config that only has a password but an empty username (the state
/// produced by the old reset_password bug) must NOT let 'admin' in.
#[tokio::test]
async fn test_empty_username_cannot_login() {
    let _guard = SERIAL.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let tc = TempConfig::new("empty-user");
    let state = new_state();

    let mut conf = Config::default();
    conf.User.Username = "".to_string(); // empty username
    conf.User.Password = ddns_rs_core::password::hash("admin12345").unwrap();
    save_config(&conf).unwrap();

    let json = call_login(&state, "admin", "admin12345").await;
    assert_eq!(json["Code"], 500, "admin must not log in when username is empty: {}", json);
}
