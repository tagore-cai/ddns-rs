use super::*;
use std::collections::BTreeMap;

/// Aliyun HMAC signature. `vals` are the request params.
pub fn aliyun_sign(secret_id: &str, secret_key: &str, vals: &mut BTreeMap<String, String>, http_method: &str, api_version: &str) {
    // Public params
    vals.insert("Format".into(), "JSON".into());
    vals.insert("Version".into(), api_version.into());
    vals.insert("AccessKeyId".into(), secret_id.into());
    vals.insert("SignatureMethod".into(), "HMAC-SHA1".into());
    vals.insert("Timestamp".into(), jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string());
    vals.insert("SignatureVersion".into(), "1.0".into());
    vals.insert("SignatureNonce".into(), uuid_nonce());

    // Sort keys
    let sorted: BTreeMap<_, _> = vals.iter().collect();
    let canonical = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let string_to_sign = format!("{}&%2F&{}", http_method, percent_encode(&canonical));

    let signature = hmac_sha1_base64(&string_to_sign, &format!("{}&", secret_key));
    vals.insert("Signature".into(), signature);
}

/// Aliyun-style HMAC-SHA1 signer over a GET query string (used by tnethk).
/// Returns the final sorted query string including the Signature param.
pub fn aliyun_style_query_sign(
    secret_id: &str,
    secret_key: &str,
    params: &mut BTreeMap<String, String>,
) -> String {
    params.insert("AccessInstanceID".into(), secret_id.into());
    params.insert("SignatureMethod".into(), "HMAC-SHA1".into());
    params.insert(
        "SignatureNonce".into(),
        jiff::Timestamp::now().as_nanosecond().to_string(),
    );
    params.insert(
        "Timestamp".into(),
        jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string(),
    );

    // Canonical query (sorted, excluding Signature)
    let canonical = params
        .iter()
        .filter(|(k, _)| k.as_str() != "Signature")
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let string_to_sign = format!("GET&{}&{}", percent_encode("/"), percent_encode(&canonical));

    let signature = hmac_sha1_base64(&string_to_sign, &format!("{}&", secret_key));
    params.insert("Signature".into(), signature);

    // Final sorted query including Signature
    params
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}
