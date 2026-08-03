use super::*;

/// Baidu Cloud BCE signer. Simplified AWS-style, only used for POST to
/// bcd.baidubce.com with the host header.
pub struct BaiduSigner {
    pub access_key_id: String,
    pub access_secret: String,
}
const BAIDU_EXPIRATION: &str = "1800";

impl BaiduSigner {
    pub fn new(access_key_id: &str, access_secret: &str) -> Self {
        Self {
            access_key_id: access_key_id.to_string(),
            access_secret: access_secret.to_string(),
        }
    }

    /// Returns the Authorization header value.
    pub fn sign(&self, method: &str, url_path: &str) -> String {
        let now = jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string();
        let auth_string_prefix =
            format!("bce-auth-v1/{}/{}/{}", self.access_key_id, now, BAIDU_EXPIRATION);

        // Canonical URI without trailing slash
        let mut canonical_url = canonical_uri(url_path);
        if canonical_url.ends_with('/') && canonical_url.len() > 1 {
            canonical_url.pop();
        }

        let canonical_req = format!(
            "{}\n{}\n{}\n{}",
            method.to_uppercase(),
            canonical_url,
            "",
            "host:bcd.baidubce.com"
        );

        let signing_key = hmac_sha256(&auth_string_prefix, self.access_secret.as_bytes());
        let signature = hmac_sha256(&canonical_req, &signing_key);

        format!(
            "{}/host/{}",
            auth_string_prefix,
            hex_encode(&signature)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_baidu_sign_structure() {
        let signer = BaiduSigner::new("AKID", "SECRET");
        let auth = signer.sign("POST", "/v1/domain/resolve/list");
        assert!(auth.starts_with("bce-auth-v1/AKID/"), "got {}", auth);
        assert!(auth.contains("/host/"));
        // 4 parts: prefix, timestamp, expiration, host/signature
        let parts: Vec<&str> = auth.split('/').collect();
        assert_eq!(parts.len(), 6, "got {}", auth);
        assert_eq!(parts[0], "bce-auth-v1");
        assert_eq!(parts[3], "1800");
        assert_eq!(parts[4], "host");
    }
}
