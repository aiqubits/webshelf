//! Balance display utilities shared across views.
//!
//! The balance is stored as a "big value" in the database:
//! 1 display unit = 10^10 stored units.

/// Balance scale factor: 1 display unit = 10^10 stored units (1 × 10^10).
pub const BALANCE_SCALE: i64 = 10_000_000_000;

/// Format stored balance to display string, truncated to 2 decimal places.
pub fn format_balance(stored: i64) -> String {
    // Divide by 10^8 to get value in cents (100 per display unit), truncating extra decimals
    let cents = stored / (BALANCE_SCALE / 100);
    let sign = if cents < 0 { "-" } else { "" };
    let abs_cents = cents.unsigned_abs();
    let integer = abs_cents / 100;
    let fraction = abs_cents % 100;
    format!("{}{}.{:02}", sign, integer, fraction)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_balance_zero() {
        assert_eq!(format_balance(0), "0.00");
    }

    #[test]
    fn test_format_balance_one_unit() {
        assert_eq!(format_balance(BALANCE_SCALE), "1.00");
    }

    #[test]
    fn test_format_balance_half_unit() {
        assert_eq!(format_balance(BALANCE_SCALE / 2), "0.50");
    }

    #[test]
    fn test_format_balance_quarter_unit() {
        assert_eq!(format_balance(BALANCE_SCALE / 4), "0.25");
    }

    #[test]
    fn test_format_balance_small_value() {
        assert_eq!(format_balance(1), "0.00");
    }

    #[test]
    fn test_format_balance_large_value() {
        assert_eq!(format_balance(BALANCE_SCALE * 123), "123.00");
    }

    #[test]
    fn test_format_balance_truncation() {
        // 1 display unit = 10^10 stored; 10^7 stored = 0.001 display, truncated to 0.00
        let stored = BALANCE_SCALE / 10_000; // 0.0001 display unit
        assert_eq!(format_balance(stored), "0.00");
    }

    #[test]
    fn test_format_balance_two_decimals() {
        // 1_234_567_890 stored ≈ 0.123456789 display → truncated 0.12
        let stored = BALANCE_SCALE / 8; // 0.125 → 0.12
        assert_eq!(format_balance(stored), "0.12");
    }

    #[test]
    fn test_format_balance_negative_value() {
        // Large negative stored would produce --X.XX double-negative in naive implementation
        let stored = -(BALANCE_SCALE * 12 + BALANCE_SCALE / 8); // -12.125 display
        assert_eq!(format_balance(stored), "-12.12");
    }

    #[test]
    fn test_format_balance_negative_small() {
        let stored = -(BALANCE_SCALE / 2); // -0.50 display
        assert_eq!(format_balance(stored), "-0.50");
    }
}
