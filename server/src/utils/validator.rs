use crate::utils::error::ApiError;

/// Validate password:
/// - At least one lowercase letter
/// - At least one uppercase letter  
/// - At least one digit
/// - At least one special character (ASCII punctuation)
/// - Minimum 8 characters
pub fn validate_password(password: &str) -> bool {
    if password.len() < 8 {
        return false;
    }

    let has_lowercase = password.chars().any(|c| c.is_ascii_lowercase());
    let has_uppercase = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    let has_special = password.chars().any(|c| c.is_ascii_punctuation());

    has_lowercase && has_uppercase && has_digit && has_special
}

/// Validate password strength, returning a descriptive error on failure.
pub fn require_password(password: &str) -> Result<(), String> {
    if validate_password(password) {
        Ok(())
    } else {
        Err("Password must be at least 8 characters and contain at least one uppercase letter, one lowercase letter, one digit, and one special character".to_string())
    }
}

/// Validate password strength and convert to ApiError on failure.
///
/// Convenience wrapper around `require_password` that directly returns
/// `ApiError::Validation`, eliminating repeated error-mapping in handlers.
pub fn check_password_strength(password: &str) -> Result<(), ApiError> {
    require_password(password).map_err(ApiError::Validation)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_passwords() {
        assert!(validate_password("Password1!"));
        assert!(validate_password("Secure@Pass123"));
        assert!(validate_password("MyP@ssw0rd"));
    }

    #[test]
    fn test_invalid_passwords() {
        assert!(!validate_password("password")); // No uppercase, no digit, no special
        assert!(!validate_password("PASSWORD1")); // No lowercase, no special
        assert!(!validate_password("Password")); // No digit, no special
        assert!(!validate_password("Password1")); // No special character
        assert!(!validate_password("Pass1!")); // Too short
    }

    #[test]
    fn test_require_password_ok() {
        assert!(require_password("Password1!").is_ok());
    }

    #[test]
    fn test_require_password_err() {
        let err = require_password("weak").unwrap_err();
        assert!(err.contains("at least 8 characters"));
        assert!(err.contains("uppercase"));
        assert!(err.contains("lowercase"));
        assert!(err.contains("digit"));
        assert!(err.contains("special character"));
    }
}
