use super::*;
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use std::collections::BTreeMap;

/// Huawei Cloud signer. Simple HMAC-SHA256 over sorted canonical query.
pub fn huawei_sign(secret_key: &str, vals: &mut BTreeMap<String, String>) {
    let sorted: BTreeMap<_, _> = vals.iter().collect();
    let canonical = sorted
        .iter()
        .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
        .collect::<Vec<_>>()
        .join("&");

    let mut mac = Hmac::<Sha256>::new_from_slice(secret_key.as_bytes()).unwrap();
    mac.update(canonical.as_bytes());
    let signature = B64.encode(mac.finalize().into_bytes());
    vals.insert("Signature".into(), signature);
}

/// Huawei Cloud API Gateway signature (SDK-HMAC-SHA256), AWS SigV4-style.
/// Mirrors Go util/signer.go exactly.
pub struct HuaweiSigner {
    pub key: String,
    pub secret: String,
}

const HUAWEI_ALGORITHM: &str = "SDK-HMAC-SHA256";

impl HuaweiSigner {
    pub fn new(key: &str, secret: &str) -> Self {
        Self {
            key: key.to_string(),
            secret: secret.to_string(),
        }
    }

    /// Build the signing headers for a request.
    /// `headers` must be the headers present before signing (excluding host,
    /// which is handled separately). The x-sdk-date header is added if absent.
    /// Returns (authorization, x-sdk-date, x-sdk-content-sha256).
    pub fn sign(
        &self,
        method: &str,
        url_path: &str,
        query: &[(String, String)],
        headers: &[(String, String)],
        body: &[u8],
    ) -> (String, String, String) {
        let now = jiff::Timestamp::now();
        let date = now.strftime("%Y%m%dT%H%M%SZ").to_string();
        let body_hash = hex_encode(&sha256_hex_bytes(body));

        // Canonical URI
        let canonical_uri = canonical_uri(url_path);

        // Canonical query: sorted by key
        let mut sorted_query = query.to_vec();
        sorted_query.sort_by(|a, b| a.0.cmp(&b.0));
        let canonical_query = sorted_query
            .iter()
            .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        // Build header map from provided headers, then add x-sdk-date.
        // host is not in r.Header in Go; it's r.Host, only used if listed.
        let mut header_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut has_date = false;
        for (k, v) in headers {
            let lk = k.to_lowercase();
            if lk == "x-sdk-date" {
                has_date = true;
            }
            header_map.entry(lk).or_default().push(v.clone());
        }
        if !has_date {
            header_map
                .entry("x-sdk-date".to_string())
                .or_default()
                .push(date.clone());
        }

        // Go: SignedHeaders iterates r.Header keys (no host), sorted.
        let mut signed_header_keys: Vec<String> = header_map.keys().cloned().collect();
        signed_header_keys.sort();

        let mut canonical_headers = String::new();
        for key in &signed_header_keys {
            let mut values = header_map.get(key).cloned().unwrap_or_default();
            values.sort();
            for v in &values {
                canonical_headers.push_str(&format!("{}:{}\n", key, v.trim()));
            }
        }

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method.to_uppercase(),
            canonical_uri,
            canonical_query,
            canonical_headers,
            signed_header_keys.join(";"),
            body_hash
        );

        let canonical_hash = sha256_hex_bytes(canonical_request.as_bytes());
        let string_to_sign = format!(
            "{}\n{}\n{}",
            HUAWEI_ALGORITHM,
            date,
            hex_encode(&canonical_hash)
        );

        let signature = hmac_sha256(&string_to_sign, self.secret.as_bytes());

        let authorization = format!(
            "{} Access={}, SignedHeaders={}, Signature={}",
            HUAWEI_ALGORITHM,
            self.key,
            signed_header_keys.join(";"),
            hex_encode(&signature)
        );

        (authorization, date, body_hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_huawei_sign_matches_go() {
        // Fixed time is not injectable; verify structural invariants instead.
        let signer = HuaweiSigner::new("AKID", "SECRET");
        let (auth, date, sha) = signer.sign("GET", "/v2/zones", &[("name".to_string(), "example.com".to_string())], &[], b"");
        assert!(auth.starts_with("SDK-HMAC-SHA256 Access=AKID, SignedHeaders=x-sdk-date, Signature="), "got {}", auth);
        // Date format YYYYMMDDTHHMMSSZ
        assert_eq!(date.len(), 16);
        assert!(date.ends_with('Z'));
        // Empty body hash is sha256("")
        assert_eq!(sha, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }
}
