use ddns_rs_core::config;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::LazyLock;
use std::time::Instant;

pub const COOKIE_NAME: &str = "token";
pub static START_TIME: LazyLock<Instant> = LazyLock::new(Instant::now);

pub struct AppState {
    pub config: Arc<Mutex<config::Config>>,
    pub cookie: Arc<Mutex<Option<String>>>,
    pub login_failures: Arc<Mutex<u32>>,
    pub lock_until: Arc<Mutex<Option<Instant>>>,
}

#[derive(Clone)]
pub struct SharedState(pub Arc<AppState>);

pub fn new_state() -> SharedState {
    SharedState(Arc::new(AppState {
        config: Arc::new(Mutex::new(config::Config::default())),
        cookie: Arc::new(Mutex::new(None)),
        login_failures: Arc::new(Mutex::new(0)),
        lock_until: Arc::new(Mutex::new(None)),
    }))
}
