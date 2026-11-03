//! PIN policy: 6 to 12 digits, no trivially guessable sequences.

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PinError {
    #[error("le code doit contenir entre 6 et 12 chiffres")]
    Length,
    #[error("le code ne doit contenir que des chiffres")]
    NotDigits,
    #[error("le code est trop simple (chiffre répété)")]
    Repeated,
    #[error("le code est trop simple (suite de chiffres)")]
    Sequence,
    #[error("le code est trop simple (motif répété)")]
    Pattern,
    #[error("le code ressemble à une date")]
    Date,
}

pub const MIN_LEN: usize = 6;
pub const MAX_LEN: usize = 12;

/// Validate a new PIN. Existing PINs are never re-validated at unlock.
pub fn validate(pin: &str) -> Result<(), PinError> {
    if !pin.chars().all(|c| c.is_ascii_digit()) {
        return Err(PinError::NotDigits);
    }
    let len = pin.len();
    if !(MIN_LEN..=MAX_LEN).contains(&len) {
        return Err(PinError::Length);
    }
    let d: Vec<u8> = pin.bytes().map(|b| b - b'0').collect();
    if d.iter().all(|&x| x == d[0]) {
        return Err(PinError::Repeated);
    }
    let asc = d.windows(2).all(|w| w[1] == (w[0] + 1) % 10);
    let desc = d.windows(2).all(|w| w[0] == (w[1] + 1) % 10);
    if asc || desc {
        return Err(PinError::Sequence);
    }
    // Short repeating motif: 121212, 123123, 12341234...
    for period in 2..=len / 2 {
        if len % period == 0 && d.chunks(period).all(|c| c == &d[..period]) {
            return Err(PinError::Pattern);
        }
    }
    if looks_like_date(&d) {
        return Err(PinError::Date);
    }
    Ok(())
}

fn num(d: &[u8]) -> u32 {
    d.iter().fold(0, |acc, &x| acc * 10 + x as u32)
}

fn valid_dm(day: u32, month: u32) -> bool {
    (1..=31).contains(&day) && (1..=12).contains(&month)
}

fn plausible_year(y: u32) -> bool {
    (1900..=2099).contains(&y)
}

/// DDMMYYYY, YYYYMMDD, MMDDYYYY (8 digits) and DDMMYY (6 digits).
fn looks_like_date(d: &[u8]) -> bool {
    match d.len() {
        8 => {
            let (a, b, y) = (num(&d[0..2]), num(&d[2..4]), num(&d[4..8]));
            let ymd = (num(&d[0..4]), num(&d[4..6]), num(&d[6..8]));
            (plausible_year(y) && (valid_dm(a, b) || valid_dm(b, a)))
                || (plausible_year(ymd.0) && valid_dm(ymd.2, ymd.1))
        }
        // DDMMYY: about 3.7 % of 6-digit codes, an acceptable rejection rate.
        6 => valid_dm(num(&d[0..2]), num(&d[2..4])),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length_and_digits() {
        assert_eq!(validate("12a456"), Err(PinError::NotDigits));
        assert_eq!(validate("83920"), Err(PinError::Length));
        assert_eq!(validate("8392011223344"), Err(PinError::Length));
    }

    #[test]
    fn trivial_codes_rejected() {
        assert_eq!(validate("000000"), Err(PinError::Repeated));
        assert_eq!(validate("123456"), Err(PinError::Sequence));
        assert_eq!(validate("987654"), Err(PinError::Sequence));
        assert_eq!(validate("890123"), Err(PinError::Sequence));
        assert_eq!(validate("121212"), Err(PinError::Pattern));
        assert_eq!(validate("12341234"), Err(PinError::Pattern));
        assert_eq!(validate("14071989"), Err(PinError::Date));
        assert_eq!(validate("19890714"), Err(PinError::Date));
    }

    #[test]
    fn reasonable_codes_accepted() {
        assert_eq!(validate("739204"), Ok(()));
        assert_eq!(validate("58204716"), Ok(()));
        assert_eq!(validate("940371862519"), Ok(()));
    }
}
