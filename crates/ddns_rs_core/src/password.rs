use bcrypt::{hash as bcrypt_hash, verify as bcrypt_verify, DEFAULT_COST};

/// Hash a password with bcrypt.
pub fn hash(plain: &str) -> Result<String, String> {
    bcrypt_hash(plain, DEFAULT_COST).map_err(|e| e.to_string())
}

/// Verify a password against a bcrypt hash.
pub fn verify(plain: &str, hashed: &str) -> bool {
    bcrypt_verify(plain, hashed).unwrap_or(false)
}

/// Check whether a stored password is already bcrypt-hashed.
pub fn is_hashed_password(pwd: &str) -> bool {
    pwd.starts_with("$2a$") || pwd.starts_with("$2b$") || pwd.starts_with("$2y$")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let h = hash("admin12345").unwrap();
        assert!(h.starts_with('$'), "hash should be bcrypt: {}", h);
        assert!(verify("admin12345", &h));
        assert!(!verify("wrong", &h));
        assert!(!verify("", &h));
    }

    #[test]
    fn test_verify_false_on_garbage() {
        // Malformed hashes must return false, not panic.
        assert!(!verify("anything", "not-a-hash"));
        assert!(!verify("anything", ""));
        assert!(!verify("anything", "$2a$10$short"));
    }

    #[test]
    fn test_is_hashed_password() {
        assert!(is_hashed_password("$2a$10$abcdef"));
        assert!(is_hashed_password("$2b$12$abcdef"));
        assert!(is_hashed_password("$2y$10$abcdef"));
        assert!(!is_hashed_password("admin12345"));
        assert!(!is_hashed_password(""));
        assert!(!is_hashed_password("$2x$10$abcdef"));
    }

    #[test]
    fn test_luci_default_hash_roundtrip() {
        // The exact hash shipped by the LuCI plugin must verify admin12345.
        let hash = "$2a$10$G1xO1cVUYtSpPYwV/Jk3l.u7PxLUxo03wntWG6VA9BxAftNWfZEhK";
        assert!(verify("admin12345", hash));
    }
}
