//! Shared signing utilities and per-vendor signers.

pub mod aliyun;
pub mod baidu;
pub mod huawei;
pub mod tencent;
pub mod volcengine;

// Re-export the per-vendor signers at the signer root so callers can keep
// using `ddns_rs_core::signer::aliyun_sign` etc.
pub use aliyun::{aliyun_sign, aliyun_style_query_sign};
pub use baidu::BaiduSigner;
pub use huawei::{huawei_sign, HuaweiSigner};
pub use tencent::TencentSigner;
pub use volcengine::TrafficRouteSigner;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use hmac::{Hmac, KeyInit, Mac};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use sha1::Sha1;
use sha2::{Digest, Sha256};

/// RFC3986 unreserved chars not percent-encoded.
const FRAGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b':')
    .add(b'@')
    .add(b'\\')
    .add(b'[')
    .add(b']')
    .add(b'!')
    .add(b'$')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b';')
    .add(b'=');

pub fn percent_encode(s: &str) -> String {
    utf8_percent_encode(s, FRAGMENT).to_string()
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

pub(crate) fn sha256_hex(s: &str) -> String {
    hex_encode(&Sha256::digest(s.as_bytes()))
}

pub(crate) fn sha256_hex_bytes(bytes: &[u8]) -> Vec<u8> {
    Sha256::digest(bytes).to_vec()
}

pub(crate) fn hmac_sha256(data: &str, key: &[u8]) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

pub(crate) fn hmac_sha1_base64(data: &str, key: &str) -> String {
    let mut mac = Hmac::<Sha1>::new_from_slice(key.as_bytes()).unwrap();
    mac.update(data.as_bytes());
    B64.encode(mac.finalize().into_bytes())
}

/// Canonical URI: split on '/', percent-encode each part, ensure trailing '/'.
pub(crate) fn canonical_uri(path: &str) -> String {
    let patterns: Vec<&str> = path.split('/').collect();
    let mut uri: Vec<String> = Vec::new();
    for v in patterns {
        uri.push(percent_encode(v));
    }
    let mut urlpath = uri.join("/");
    if urlpath.is_empty() || !urlpath.ends_with('/') {
        urlpath.push('/');
    }
    urlpath
}

pub(crate) fn uuid_nonce() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    hex_encode(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canonical_uri() {
        assert_eq!(canonical_uri("/v2/zones"), "/v2/zones/");
        assert_eq!(canonical_uri("/v2/zones/"), "/v2/zones/");
        assert_eq!(canonical_uri("/v2.1/zones/abc/recordsets"), "/v2.1/zones/abc/recordsets/");
    }
}


