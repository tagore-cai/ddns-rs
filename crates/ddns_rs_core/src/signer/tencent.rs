use super::*;

/// Tencent Cloud TC3-HMAC-SHA256 signature v3.
pub struct TencentSigner {
    pub secret_id: String,
    pub secret_key: String,
}

impl TencentSigner {
    pub fn new(secret_id: &str, secret_key: &str) -> Self {
        Self {
            secret_id: secret_id.to_string(),
            secret_key: secret_key.to_string(),
        }
    }

    pub fn sign(&self, host: &str, action: &str, payload: &str) -> (String, String, String, String) {
        let algorithm = "TC3-HMAC-SHA256";
        let timestamp = jiff::Timestamp::now().as_second();
        let timestamp_str = timestamp.to_string();

        let canonical_headers = format!(
            "content-type:application/json\nhost:{}\nx-tc-action:{}\n",
            host,
            action.to_lowercase()
        );
        let signed_headers = "content-type;host;x-tc-action";
        let hashed_payload = sha256_hex(payload);
        let canonical_request = format!("POST\n/\n\n{}\n{}\n{}", canonical_headers, signed_headers, hashed_payload);

        let date = jiff::Timestamp::now().strftime("%Y-%m-%d").to_string();
        let service = host
            .split('.')
            .next()
            .unwrap_or("dnspod")
            .to_string();
        let credential_scope = format!("{}/{}/tc3_request", date, service);
        let hashed_canonical_request = sha256_hex(&canonical_request);
        let string_to_sign = format!(
            "{}\n{}\n{}\n{}",
            algorithm, timestamp_str, credential_scope, hashed_canonical_request
        );

        let secret_date = hmac_sha256(&date, format!("TC3{}", self.secret_key).as_bytes());
        let secret_service = hmac_sha256(&service, &secret_date);
        let secret_signing = hmac_sha256("tc3_request", &secret_service);
        let signature = hex_encode(&hmac_sha256(&string_to_sign, &secret_signing));

        let authorization = format!(
            "{} Credential={}/{}, SignedHeaders={}, Signature={}",
            algorithm, self.secret_id, credential_scope, signed_headers, signature
        );

        (
            authorization,
            host.to_string(),
            action.to_string(),
            timestamp_str,
        )
    }
}
