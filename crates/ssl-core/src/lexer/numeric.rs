use super::token::{NumericBase, NumericLiteral};

/// Parse a numeric literal string into a NumericLiteral.
///
/// Formats:
/// - Decimal: `42`, `1_000_000`
/// - Hex: `0xFF`, `0xDEAD_BEEF`
/// - Binary: `0b1010_0011`
/// - Sized: `8'b1010_0011`, `16'hDEAD`, `8'd255`
///
/// Underscores are allowed anywhere after the prefix for readability.
/// Don't-care bits (`?`) are only valid in sized binary literals.
pub(crate) fn parse_numeric(s: &str) -> Option<NumericLiteral> {
    if let Some(tick_pos) = s.find('\'') {
        let width_str = &s[..tick_pos];
        let width: u32 = width_str.parse().ok()?;
        let rest = &s[tick_pos + 1..];

        if rest.is_empty() {
            return None;
        }

        if width == 0 || width > 128 {
            return None;
        }

        let (base, digits) = match rest.as_bytes()[0] {
            b'b' | b'B' => (NumericBase::Binary, &rest[1..]),
            b'h' | b'H' => (NumericBase::Hex, &rest[1..]),
            b'd' | b'D' => (NumericBase::Decimal, &rest[1..]),
            _ => return None,
        };

        let stripped: Vec<char> = digits.chars().filter(|&c| c != '_').collect();

        let radix = match base {
            NumericBase::Binary => 2,
            NumericBase::Decimal => 10,
            NumericBase::Hex => 16,
        };

        // Don't-care (`?`) is only valid in binary literals.
        if base != NumericBase::Binary && stripped.contains(&'?') {
            return None;
        }

        let mut dont_care_mask: u128 = 0;
        if base == NumericBase::Binary {
            for &ch in &stripped {
                dont_care_mask <<= 1;
                if ch == '?' {
                    dont_care_mask |= 1;
                }
            }
        }

        let clean: String = stripped
            .iter()
            .map(|&c| if c == '?' { '0' } else { c })
            .collect();

        let value = u128::from_str_radix(&clean, radix).ok()?;

        if width < 128 && value >= (1u128 << width) {
            return None;
        }

        return Some(NumericLiteral::Sized { width, value, base, dont_care_mask });
    }

    let clean: String = s.chars().filter(|&c| c != '_').collect();

    if let Some(hex) = clean.strip_prefix("0x").or_else(|| clean.strip_prefix("0X")) {
        let value = u128::from_str_radix(hex, 16).ok()?;
        Some(NumericLiteral::Hex(value))
    } else if let Some(bin) = clean.strip_prefix("0b").or_else(|| clean.strip_prefix("0B")) {
        let value = u128::from_str_radix(bin, 2).ok()?;
        Some(NumericLiteral::Binary(value))
    } else {
        let value: u128 = clean.parse().ok()?;
        Some(NumericLiteral::Decimal(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_literal() {
        assert_eq!(parse_numeric("42"), Some(NumericLiteral::Decimal(42)));
    }

    #[test]
    fn decimal_with_underscores() {
        assert_eq!(parse_numeric("1_000_000"), Some(NumericLiteral::Decimal(1_000_000)));
    }

    #[test]
    fn hex_literal() {
        assert_eq!(parse_numeric("0xFF"), Some(NumericLiteral::Hex(0xFF)));
    }

    #[test]
    fn hex_literal_upper() {
        assert_eq!(parse_numeric("0xDEAD_BEEF"), Some(NumericLiteral::Hex(0xDEAD_BEEF)));
    }

    #[test]
    fn binary_literal() {
        assert_eq!(parse_numeric("0b1010_0011"), Some(NumericLiteral::Binary(0b1010_0011)));
    }

    #[test]
    fn sized_binary() {
        assert_eq!(
            parse_numeric("8'b1010_0011"),
            Some(NumericLiteral::Sized { width: 8, value: 0b1010_0011, base: NumericBase::Binary, dont_care_mask: 0 })
        );
    }

    #[test]
    fn sized_hex() {
        assert_eq!(
            parse_numeric("16'hDEAD"),
            Some(NumericLiteral::Sized { width: 16, value: 0xDEAD, base: NumericBase::Hex, dont_care_mask: 0 })
        );
    }

    #[test]
    fn sized_decimal() {
        assert_eq!(
            parse_numeric("8'd255"),
            Some(NumericLiteral::Sized { width: 8, value: 255, base: NumericBase::Decimal, dont_care_mask: 0 })
        );
    }

    #[test]
    fn sized_with_dont_care() {
        assert_eq!(
            parse_numeric("4'b10??"),
            Some(NumericLiteral::Sized { width: 4, value: 0b1000, base: NumericBase::Binary, dont_care_mask: 0b0011 })
        );
    }

    #[test]
    fn sized_value_too_large() {
        assert_eq!(parse_numeric("4'hFF"), None);
    }

    #[test]
    fn zero() {
        assert_eq!(parse_numeric("0"), Some(NumericLiteral::Decimal(0)));
    }

    #[test]
    fn width_zero() {
        assert_eq!(parse_numeric("0'b0"), None);
    }

    #[test]
    fn width_too_large() {
        assert_eq!(parse_numeric("200'hFF"), None);
    }

    #[test]
    fn hex_no_dont_care() {
        // After fix, the regex won't match `8'hF?` at all (no `?` in hex pattern),
        // so it never reaches parse_numeric. Verify parse_numeric itself also rejects it
        // if somehow called directly: `?` is not a valid hex digit, so from_str_radix fails.
        assert_eq!(parse_numeric("8'hF?"), None);
    }
}
