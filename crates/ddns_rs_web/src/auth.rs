use crate::handlers;
use crate::state::{SharedState, COOKIE_NAME};
use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::Response;
use std::net::SocketAddr;

fn authorized(state: &SharedState, cookie_header: Option<&str>, remote: &str) -> bool {
    // 禁止公网访问 (mirrors Go's Auth middleware)
    if let Ok(conf) = ddns_rs_core::config::get_config_cached() {
        if conf.NotAllowWanAccess && !ddns_rs_core::netutil::is_private_network(remote) {
            ddns_rs_core::log_msg!("%q 被禁止从公网访问", remote);
            return false;
        }
    }
    if let Some(cookie_val) = cookie_header {
        if let Some(token) = cookie_val
            .split(';')
            .map(|s| s.trim())
            .find_map(|s| s.strip_prefix(&format!("{}=", COOKIE_NAME)))
        {
            if let Some(system) = state.0.cookie.lock().unwrap().clone() {
                return token == system;
            }
        }
    }
    false
}

fn auth_forbidden() -> Response {
    Response::builder()
        .status(StatusCode::FORBIDDEN)
        .body(Body::empty())
        .unwrap()
}

/// Determine whether the auth failure was due to a blocked public address.
fn is_public_and_blocked(remote: &str) -> bool {
    if let Ok(conf) = ddns_rs_core::config::get_config_cached() {
        if conf.NotAllowWanAccess && !ddns_rs_core::netutil::is_private_network(remote) {
            return true;
        }
    }
    false
}

fn redirect_login() -> Response {
    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "./login")
        .body(Body::empty())
        .unwrap()
}

pub async fn static_handler(
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    Path(path): Path<String>,
) -> Response {
    let remote = addr.to_string();
    // AuthAssert-style: block WAN access when config forbids it.
    if let Ok(conf) = ddns_rs_core::config::get_config_cached() {
        if conf.NotAllowWanAccess && !ddns_rs_core::netutil::is_private_network(&remote) {
            ddns_rs_core::log_msg!("%q 被禁止从公网访问", remote);
            return auth_forbidden();
        }
    }
    crate::assets::serve_static(&path).await
}

pub async fn save_authed(
    state: SharedState,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    let remote = addr.to_string();
    if !authorized(&state, headers.get(header::COOKIE).and_then(|v| v.to_str().ok()), &remote) {
        if is_public_and_blocked(&remote) {
            return auth_forbidden();
        }
        return redirect_login();
    }
    handlers::save(state, body).await
}

pub async fn set_lang_authed(
    state: SharedState,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    let remote = addr.to_string();
    if !authorized(&state, headers.get(header::COOKIE).and_then(|v| v.to_str().ok()), &remote) {
        if is_public_and_blocked(&remote) {
            return auth_forbidden();
        }
        return redirect_login();
    }
    handlers::set_lang(state, body).await
}

pub async fn logs_authed(
    state: SharedState,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> Response {
    let remote = addr.to_string();
    if !authorized(&state, headers.get(header::COOKIE).and_then(|v| v.to_str().ok()), &remote) {
        if is_public_and_blocked(&remote) {
            return auth_forbidden();
        }
        return redirect_login();
    }
    handlers::logs().await
}

pub async fn clear_log_authed(
    state: SharedState,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
) -> Response {
    let remote = addr.to_string();
    if !authorized(&state, headers.get(header::COOKIE).and_then(|v| v.to_str().ok()), &remote) {
        if is_public_and_blocked(&remote) {
            return auth_forbidden();
        }
        return redirect_login();
    }
    handlers::clear_log().await
}

pub async fn webhook_test_authed(
    state: SharedState,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    body: String,
) -> Response {
    let remote = addr.to_string();
    if !authorized(&state, headers.get(header::COOKIE).and_then(|v| v.to_str().ok()), &remote) {
        if is_public_and_blocked(&remote) {
            return auth_forbidden();
        }
        return redirect_login();
    }
    handlers::webhook_test(state, body).await
}
