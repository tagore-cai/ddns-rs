use super::*;

/// Volcengine (火山引擎) TrafficRoute signer. AWS SigV4-style, fixed host/region/service.
pub struct TrafficRouteSigner {
    pub access_key_id: String,
    pub secret_access_key: String,
}

const TR_HOST: &str = "open.volcengineapi.com";
const TR_SERVICE: &str = "DNS";
const TR_REGION: &str = "cn-north-1";
const TR_VERSION: &str = "2018-08-01";

impl TrafficRouteSigner {
    pub fn new(access_key_id: &str, secret_access_key: &str) -> Self {
        Self {
            access_key_id: access_key_id.to_string(),
            secret_access_key: secret_access_key.to_string(),
        }
    }

    /// Returns (authorization, x-date, x-content-sha256, host, content-type).
    pub fn sign(
        &self,
        method: &str,
        query: &[(String, String)],
        action: &str,
        body: &[u8],
    ) -> (String, String, String, String, String) {
        let now = jiff::Timestamp::now();
        let x_date = now.strftime("%Y%m%dT%H%M%SZ").to_string();
        let short_x_date = &x_date[..8];
        let x_content_sha256 = hex_encode(&sha256_hex_bytes(body));

        // Build query with Action and Version
        let mut q: Vec<(String, String)> = query.to_vec();
        q.push(("Action".to_string(), action.to_string()));
        q.push(("Version".to_string(), TR_VERSION.to_string()));
        q.sort_by(|a, b| a.0.cmp(&b.0));
        let raw_query: Vec<String> = q.iter().map(|(k, v)| format!("{}={}", k, v)).collect();
        let raw_query = raw_query.join("&");

        let content_type = "application/json";
        let signed_headers = "content-type;host;x-content-sha256;x-date";
        let canonical_headers = format!(
            "content-type:{}\nhost:{}\nx-content-sha256:{}\nx-date:{}\n",
            content_type, TR_HOST, x_content_sha256, x_date
        );

        let canonical_request = format!(
            "{}\n/\n{}\n{}\n{}\n{}",
            method.to_uppercase(),
            raw_query,
            canonical_headers,
            signed_headers,
            x_content_sha256
        );

        let hashed_canonical = hex_encode(&sha256_hex_bytes(canonical_request.as_bytes()));
        let credential_scope = format!("{}/{}/{}/request", short_x_date, TR_REGION, TR_SERVICE);
        let string_to_sign = format!(
            "HMAC-SHA256\n{}\n{}\n{}",
            x_date, credential_scope, hashed_canonical
        );

        let k_date = hmac_sha256(short_x_date, self.secret_access_key.as_bytes());
        let k_region = hmac_sha256(TR_REGION, &k_date);
        let k_service = hmac_sha256(TR_SERVICE, &k_region);
        let k_signing = hmac_sha256("request", &k_service);
        let signature = hex_encode(&hmac_sha256(&string_to_sign, &k_signing));

        let authorization = format!(
            "HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.access_key_id, credential_scope, signed_headers, signature
        );

        (
            authorization,
            x_date,
            x_content_sha256,
            TR_HOST.to_string(),
            content_type.to_string(),
        )
    }
}
