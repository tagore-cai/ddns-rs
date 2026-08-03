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
