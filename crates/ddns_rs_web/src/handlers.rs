use crate::assets::render_template;
use crate::dto::{dns_conf_to_js, DnsConfJS, LoginData, SaveData};
use crate::json::{return_error, return_json, return_json_raw, return_ok, generate_token};
use crate::state::{SharedState, START_TIME, COOKIE_NAME};
use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use std::time::{Duration, Instant};

pub async fn login_page(state: SharedState) -> Response {
    let conf = {
        let cache = ddns_rs_core::config::get_config_cached().unwrap_or_default();
        let mut st = state.0.config.lock().unwrap();
        *st = cache.clone();
        cache
    };
    let empty_user = conf.User.Username.is_empty() && conf.User.Password.is_empty();
    let ctx = serde_json::json!({
        "EmptyUser": empty_user,
        "Lang": conf.Lang,
    });
    render_template("login.html", &ctx)
}

pub async fn login_func(state: SharedState, Json(data): Json<LoginData>) -> Response {
    let mut conf = ddns_rs_core::config::get_config_cached().unwrap_or_default();
    {
        let mut st = state.0.config.lock().unwrap();
        *st = conf.clone();
    }

    // lock check
    {
        let lock_until = state.0.lock_until.lock().unwrap();
        if let Some(until) = *lock_until {
            if Instant::now() < until {
                return return_error(&ddns_rs_core::logger::t("登录失败次数过多，请稍后再试", &[]));
            }
        }
    }

    if data.username.is_empty() || data.password.is_empty() {
        return return_error(&ddns_rs_core::logger::t("必须输入用户名/密码", &[]));
    }

    // init username/password
    if conf.User.Username.is_empty() && conf.User.Password.is_empty() {
        if START_TIME.elapsed() > Duration::from_secs(30 * 60) {
            let secs = (*START_TIME + Duration::from_secs(30 * 60))
                .duration_since(Instant::now())
                .as_secs()
                .to_string();
            return return_error(&ddns_rs_core::logger::t(
                "需在 %s 之前完成用户名密码设置,请重启ddns-go",
                &[&secs],
            ));
        }
        conf.User.Username = data.username.clone();
        match ddns_rs_core::config::check_password(&data.password, true) {
            Ok(hashed) => conf.User.Password = hashed,
            Err(e) => return return_error(&e),
        }
        conf.NotAllowWanAccess = true;
        if let Err(e) = ddns_rs_core::config::save_config(&conf) {
            return return_error(&e);
        }
    }

    if data.username == conf.User.Username && ddns_rs_core::password::verify(&data.password, &conf.User.Password) {
        let mut failures = state.0.login_failures.lock().unwrap();
        *failures = 0;
        *state.0.lock_until.lock().unwrap() = None;

        let token = generate_token(&data.username);
        *state.0.cookie.lock().unwrap() = Some(token.clone());
        let timeout_days = if conf.NotAllowWanAccess { 30 } else { 1 };

        let cookie = format!(
            "{}={}; Path=/; HttpOnly; Max-Age={}",
            COOKIE_NAME, token, timeout_days * 86400
        );
        ddns_rs_core::log_msg!("%q 登录成功", "login");
        let mut resp = return_ok(&ddns_rs_core::logger::t("登录成功", &[]), Some(token));
        resp.headers_mut()
            .insert(header::SET_COOKIE, cookie.parse().unwrap());
        resp
    } else {
        let mut failures = state.0.login_failures.lock().unwrap();
        *failures += 1;
        if *failures >= 5 {
            *state.0.lock_until.lock().unwrap() = Some(Instant::now() + Duration::from_secs(30 * 60));
        }
        ddns_rs_core::log_msg!("%q 帐号密码不正确", "login");
        return_error(&ddns_rs_core::logger::t("用户名或密码错误", &[]))
    }
}

pub async fn writing_page(state: SharedState) -> Response {
    let conf = ddns_rs_core::config::get_config_cached().unwrap_or_default();
    let mut st = state.0.config.lock().unwrap();
    *st = conf.clone();
    let dns_conf = serde_json::to_string(&dns_conf_to_js(&conf)).unwrap_or("[]".into());
    let (ipv4, ipv6) = ddns_rs_core::netiface::get_net_interface().unwrap_or_default();
    let mut all_ifaces = std::collections::HashSet::new();
    for i in &ipv4 {
        all_ifaces.insert(i.name.clone());
    }
    for i in &ipv6 {
        all_ifaces.insert(i.name.clone());
    }

    let to_json = |ifaces: Vec<ddns_rs_core::netiface::NetInterface>| -> serde_json::Value {
        serde_json::to_value(
            &ifaces
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "Name": i.name,
                        "Address": i.address,
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or(serde_json::Value::Array(vec![]))
    };
    let all_ifaces_vec: Vec<ddns_rs_core::netiface::NetInterface> = all_ifaces
        .iter()
        .map(|n| ddns_rs_core::netiface::NetInterface {
            name: n.clone(),
            address: vec![],
        })
        .collect();

    let ctx = serde_json::json!({
        "Version": ddns_rs_core::VERSION,
        "DnsConf": dns_conf,
        "NotAllowWanAccess": conf.NotAllowWanAccess,
        "Username": conf.User.Username,
        "Lang": conf.Lang,
        "WebhookURL": conf.Webhook.WebhookURL,
        "WebhookRequestBody": conf.Webhook.WebhookRequestBody,
        "WebhookHeaders": conf.Webhook.WebhookHeaders,
        "Ipv4": to_json(ipv4),
        "Ipv6": to_json(ipv6),
        "AllInterfaces": to_json(all_ifaces_vec),
    });
    render_template("writing.html", &ctx)
}

pub async fn save(_state: SharedState, body: String) -> Response {
    let data: SaveData = match serde_json::from_str(&body) {
        Ok(d) => d,
        Err(_) => return return_json("数据解析失败, 请刷新页面重试", ""),
    };

    let mut conf = ddns_rs_core::config::get_config_cached().unwrap_or_default();

    if let Some(lang) = data.lang {
        if !lang.trim().is_empty() {
            conf.Lang = ddns_rs_core::logger::init_lang(&lang);
        }
    } else {
        conf.Lang = ddns_rs_core::logger::init_lang(&conf.Lang);
    }
    if let Some(v) = data.not_allow_wan_access {
        conf.NotAllowWanAccess = v;
    }
    if let Some(v) = data.webhook_url {
        conf.Webhook.WebhookURL = v.trim().to_string();
    }
    if let Some(v) = data.webhook_request_body {
        conf.Webhook.WebhookRequestBody = v.trim().to_string();
    }
    if let Some(v) = data.webhook_headers {
        conf.Webhook.WebhookHeaders = v.trim().to_string();
    }
    if let Some(u) = data.username {
        conf.User.Username = u.trim().to_string();
    }
    if let Some(p) = data.password {
        if !p.is_empty() {
            match ddns_rs_core::config::check_password(&p, conf.NotAllowWanAccess) {
                Ok(hashed) => conf.User.Password = hashed,
                Err(e) => return return_json(&e, ""),
            }
        }
    }
    if conf.User.Username.is_empty() || conf.User.Password.is_empty() {
        return return_json(&ddns_rs_core::logger::t("必须输入用户名/密码", &[]), "");
    }

    // DNS configs
    let dns_conf_js = data.dns_conf.unwrap_or_default();
    let mut dns_conf_array = Vec::new();
    let empty = DnsConfJS::default();
    let old_conf = ddns_rs_core::config::get_config_cached().unwrap_or_default();
    for (k, v) in dns_conf_js.iter().enumerate() {
        if *v == empty {
            continue;
        }
        let mut dc = ddns_rs_core::config::DnsConfig {
            Name: v.name.clone(),
            TTL: v.ttl.clone(),
            ..Default::default()
        };
        dc.DNS.Name = v.dns_name.clone();
        dc.DNS.ID = v.dns_id.trim().to_string();
        dc.DNS.Secret = v.dns_secret.trim().to_string();
        dc.DNS.ExtParam = v.dns_ext_param.trim().to_string();
        dc.Ipv4.Enable = v.ipv4_enable;
        dc.Ipv4.GetType = v.ipv4_get_type.clone();
        dc.Ipv4.URL = v.ipv4_url.trim().to_string();
        dc.Ipv4.NetInterface = v.ipv4_net_interface.clone();
        dc.Ipv4.Cmd = v.ipv4_cmd.trim().to_string();
        dc.Ipv4.Domains = split_lines(&v.ipv4_domains);
        dc.Ipv6.Enable = v.ipv6_enable;
        dc.Ipv6.GetType = v.ipv6_get_type.clone();
        dc.Ipv6.URL = v.ipv6_url.trim().to_string();
        dc.Ipv6.NetInterface = v.ipv6_net_interface.clone();
        dc.Ipv6.Cmd = v.ipv6_cmd.trim().to_string();
        dc.Ipv6.Ipv6Reg = v.ipv6_reg.trim().to_string();
        dc.Ipv6.Domains = split_lines(&v.ipv6_domains);
        dc.HttpInterface = v.http_interface.trim().to_string();

        if let Some(old) = old_conf.DnsConf.get(k) {
            let (id_hide, secret_hide) = crate::dto::hide_id_secret(&old.DNS.ID, &old.DNS.Secret, &old.DNS.Name);
            if dc.DNS.ID == id_hide {
                dc.DNS.ID = old.DNS.ID.clone();
            }
            if dc.DNS.Secret == secret_hide {
                dc.DNS.Secret = old.DNS.Secret.clone();
            }
        }
        dns_conf_array.push(dc);
    }
    conf.DnsConf = dns_conf_array;

    match ddns_rs_core::config::save_config(&conf) {
        Ok(_) => {
            // Force a compare and run once (mirrors Go: ForceCompareGlobal=true; go dns.RunOnce())
            ddns_rs_providers::engine::force_compare();
            tokio::spawn(async {
                ddns_rs_providers::engine::run_once_with(&ddns_rs_providers::PROVIDER_FACTORY).await;
            });
            let new_conf = ddns_rs_core::config::get_config_cached().unwrap_or_default();
            let dns_json = serde_json::to_string(&dns_conf_to_js(&new_conf)).unwrap_or("[]".into());
            return_json("ok", &dns_json)
        }
        Err(e) => return_json(&e, ""),
    }
}

fn split_lines(s: &str) -> Vec<String> {
    let sep = if s.contains("\r\n") { "\r\n" } else { "\n" };
    s.split(sep)
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect()
}

pub async fn logs() -> Response {
    let logs = ddns_rs_core::logger::MemoryLog::all();
    return_json_raw(&serde_json::to_string(&logs).unwrap_or("[]".into()))
}

pub async fn clear_log() -> Response {
    ddns_rs_core::logger::MemoryLog::clear();
    Response::new(Body::empty())
}

pub async fn set_lang(state: SharedState, body: String) -> Response {
    #[derive(Deserialize)]
    struct LangData {
        #[serde(rename = "Lang")]
        lang: Option<String>,
    }
    let data: LangData = serde_json::from_str(&body).unwrap_or(LangData { lang: None });
    let mut conf = ddns_rs_core::config::get_config_cached().unwrap_or_default();
    let lang = data.lang.filter(|l| !l.is_empty()).unwrap_or_else(|| conf.Lang.clone());
    conf.Lang = ddns_rs_core::logger::init_lang(&lang);
    let _ = state;
    match ddns_rs_core::config::save_config(&conf) {
        Ok(_) => return_json("ok", &conf.Lang),
        Err(e) => return_error(&e),
    }
}

pub async fn webhook_test(state: SharedState, body: String) -> Response {
    #[derive(Deserialize)]
    struct TestData {
        #[serde(rename = "URL")]
        url: Option<String>,
        #[serde(rename = "RequestBody")]
        request_body: Option<String>,
        #[serde(rename = "Headers")]
        headers: Option<String>,
    }
    let _ = state;
    let data: TestData = serde_json::from_str(&body).unwrap_or(TestData {
        url: None,
        request_body: None,
        headers: None,
    });
    let url = data.url.unwrap_or_default();
    if url.is_empty() {
        ddns_rs_core::log_msg!("请输入Webhook的URL");
        return Response::new(Body::empty());
    }
    let mut domains = ddns_rs_core::domain::Domains::default();
    let mut d = ddns_rs_core::domain::Domain::default();
    d.domain_name = "example.com".into();
    d.sub_domain = "test".into();
    d.update_status = ddns_rs_core::config::UpdateStatus::Success;
    domains.ipv4_addr = "127.0.0.1".into();
    domains.ipv6_addr = "::1".into();
    domains.ipv4_domains = vec![d.clone()];
    domains.ipv6_domains = vec![d];
    let webhook = ddns_rs_core::config::Webhook {
        WebhookURL: url,
        WebhookRequestBody: data.request_body.unwrap_or_default(),
        WebhookHeaders: data.headers.unwrap_or_default(),
    };
    ddns_rs_providers::engine::exec_webhook(&domains, &webhook).await;
    Response::new(Body::empty())
}

pub async fn logout(state: SharedState) -> Response {
    *state.0.cookie.lock().unwrap() = None;
    let cookie = format!("{}=; Path=/; Max-Age=0", COOKIE_NAME);
    let mut resp = Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "./login")
        .body(Body::empty())
        .unwrap();
    resp.headers_mut()
        .insert(header::SET_COOKIE, cookie.parse().unwrap());
    resp
}
