use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::Response;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct StaticAssets;

#[derive(RustEmbed)]
#[folder = "templates/"]
pub struct TemplateAssets;

pub async fn serve_static(path: &str) -> Response {
    if let Some(file) = StaticAssets::get(path) {
        let mime = mime_for(path);
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(file.data.into_owned()))
            .unwrap();
    }
    ddns_rs_core::logger::log_line(&format!("static not found: {:?}", path));
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::from("not found"))
        .unwrap()
}

fn mime_for(path: &str) -> &'static str {
    if path.ends_with(".css") {
        "text/css"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".png") {
        "image/png"
    } else if path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

pub fn render_template(name: &str, ctx: &serde_json::Value) -> Response {
    let content = match TemplateAssets::get(name) {
        Some(f) => String::from_utf8_lossy(&f.data).to_string(),
        None => return Response::builder().status(StatusCode::NOT_FOUND).body(Body::from("not found")).unwrap(),
    };
    let html = crate::gotemplate::render_template(&content, ctx);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(html))
        .unwrap()
}
