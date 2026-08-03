use axum::body::Body;
use axum::http::{header, StatusCode};
use axum::response::Response;

pub fn return_error(msg: &str) -> Response {
    return_json_raw(&serde_json::json!({ "Code": 500, "Msg": msg }).to_string())
}

pub fn return_ok(msg: &str, data: Option<String>) -> Response {
    let data_val = data.map(|d| serde_json::Value::String(d));
    return_json_raw(&serde_json::json!({ "Code": 200, "Msg": msg, "Data": data_val }).to_string())
}

pub fn return_json(result: &str, dns_conf: &str) -> Response {
    return_json_raw(&serde_json::json!({ "result": result, "dnsConf": dns_conf }).to_string())
}

pub fn return_json_raw(json: &str) -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json.to_string()))
        .unwrap()
}

pub fn generate_token(username: &str) -> String {
    use base64::{engine::general_purpose::STANDARD as B64, Engine};
    use rand::RngExt;
    let mut rng = rand::rng();
    let key = rng.random::<u64>().to_string();
    let ts = jiff::Timestamp::now().as_second();
    let msg = format!("{}{}", username, ts);
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::Sha256;
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(key.as_bytes()).unwrap();
    mac.update(msg.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}
