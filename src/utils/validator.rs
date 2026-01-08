use once_cell::sync::Lazy;
use regex::Regex;

/// Validate password:
/// - At least one lowercase letter
/// - At least one uppercase letter  
/// - At least one digit
/// - Minimum 8 characters
pub fn validate_password(password: &str) -> bool {
    if password.len() < 8 {
        return false;
    }
    
    let has_lowercase = password.chars().any(|c| c.is_ascii_lowercase());
    let has_uppercase = password.chars().any(|c| c.is_ascii_uppercase());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    
    has_lowercase && has_uppercase && has_digit
}

/// Email regex pattern for basic email validation
static EMAIL_REGEX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")
        .expect("Invalid email regex pattern")
});

/// Validate email format
pub fn validate_email(email: &str) -> bool {
    EMAIL_REGEX.is_match(email)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_passwords() {
        assert!(validate_password("Password1"));
        assert!(validate_password("SecurePass123"));
        assert!(validate_password("MyP@ssw0rd"));
    }

    #[test]
    fn test_invalid_passwords() {
        assert!(!validate_password("password")); // No uppercase, no digit
        assert!(!validate_password("PASSWORD1")); // No lowercase
        assert!(!validate_password("Password")); // No digit
        assert!(!validate_password("Pass1")); // Too short
    }

    #[test]
    fn test_valid_emails() {
        assert!(validate_email("user@example.com"));
        assert!(validate_email("user.name@example.co.uk"));
        assert!(validate_email("user+tag@example.org"));
    }

    #[test]
    fn test_invalid_emails() {
        assert!(!validate_email("invalid"));
        assert!(!validate_email("@example.com"));
        assert!(!validate_email("user@"));
    }
}
