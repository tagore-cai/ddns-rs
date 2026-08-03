use crate::auth;
use crate::dto::LoginData;
use crate::handlers;
use crate::state::{new_state, SharedState};
use axum::extract::Path;
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use axum::Json;
use std::net::SocketAddr;

pub fn build_router(state: SharedState) -> Router {
    let s = state.clone();
    Router::new()
        .route("/", get(move || handlers::writing_page(s.clone())))
        .route("/login", get({
            let s = state.clone();
            move || handlers::login_page(s.clone())
        }))
        .route("/loginFunc", post({
            let s = state.clone();
            move |Json(data): Json<LoginData>| handlers::login_func(s.clone(), Json(data))
        }))
        .route("/save", post({
            let s = state.clone();
            move |ci: axum::extract::ConnectInfo<SocketAddr>, headers: axum::http::HeaderMap, body: String| {
                auth::save_authed(s.clone(), ci, headers, body)
            }
        }))
        .route("/setLang", post({
            let s = state.clone();
            move |ci: axum::extract::ConnectInfo<SocketAddr>, headers: axum::http::HeaderMap, body: String| {
                auth::set_lang_authed(s.clone(), ci, headers, body)
            }
        }))
        .route("/logs", get({
            let s = state.clone();
            move |ci: axum::extract::ConnectInfo<SocketAddr>, headers: axum::http::HeaderMap| {
                auth::logs_authed(s.clone(), ci, headers)
            }
        }))
        .route("/clearLog", get({
            let s = state.clone();
            move |ci: axum::extract::ConnectInfo<SocketAddr>, headers: axum::http::HeaderMap| {
                auth::clear_log_authed(s.clone(), ci, headers)
            }
        }))
        .route("/webhookTest", post({
            let s = state.clone();
            move |ci: axum::extract::ConnectInfo<SocketAddr>, headers: axum::http::HeaderMap, body: String| {
                auth::webhook_test_authed(s.clone(), ci, headers, body)
            }
        }))
        .route("/logout", get({
            let s = state.clone();
            move || handlers::logout(s.clone())
        }))
        .route("/static/{*path}", get({
            move |ci: axum::extract::ConnectInfo<SocketAddr>, path: Path<String>| {
                auth::static_handler(ci, path)
            }
        }))
        .route("/favicon.ico", get(favicon))
}

async fn favicon() -> Response {
    crate::assets::serve_static("favicon.ico").await
}

/// Run the web server.
pub async fn run(listen: &str) -> anyhow::Result<()> {
    let addr: SocketAddr = listen.parse()?;
    let state = new_state();
    let app = build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    ddns_rs_core::log_msg!("监听 %s", listen);
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;
    Ok(())
}
