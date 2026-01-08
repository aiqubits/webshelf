use anyhow::{anyhow, Result};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};

/// Hash a password using Argon2
///
/// # Arguments
/// * `password` - Plain text password to hash
///
/// # Returns
/// * `Result<String>` - The hashed password string
pub fn hash_password(password: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();

    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow!("Failed to hash password: {}", e))?;

    Ok(password_hash.to_string())
}

/// Verify a password against a hash
///
/// # Arguments
/// * `password` - Plain text password to verify
/// * `password_hash` - The hash to verify against
///
/// # Returns
/// * `Result<bool>` - True if the password matches the hash
pub fn verify_password(password: &str, password_hash: &str) -> Result<bool> {
    let parsed_hash =
        PasswordHash::new(password_hash).map_err(|e| anyhow!("Failed to parse password hash: {}", e))?;

    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify_password() {
        let password = "SecurePassword123!";
        let hash = hash_password(password).expect("Failed to hash password");

        assert!(verify_password(password, &hash).expect("Failed to verify password"));
        assert!(!verify_password("WrongPassword", &hash).expect("Failed to verify password"));
    }

    #[test]
    fn test_different_passwords_produce_different_hashes() {
        let hash1 = hash_password("Password1").expect("Failed to hash password");
        let hash2 = hash_password("Password2").expect("Failed to hash password");

        assert_ne!(hash1, hash2);
    }
}
