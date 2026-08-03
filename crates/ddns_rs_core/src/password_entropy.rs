/// Password strength checker mirroring Go's go-password-validator library.
/// https://github.com/wagslane/go-password-validator

const REPLACE_CHARS: &str = "!@$&*";
const SEP_CHARS: &str = "_-., ";
const OTHER_SPECIAL_CHARS: &str = "\"#%'()+/:;<=>?[\\]^{|}~";
const LOWER_CHARS: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER_CHARS: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS_CHARS: &str = "0123456789";

const SEQ_NUMS: &str = "0123456789";
const SEQ_KEYBOARD0: &str = "qwertyuiop";
const SEQ_KEYBOARD1: &str = "asdfghjkl";
const SEQ_KEYBOARD2: &str = "zxcvbnm";
const SEQ_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";

/// Compute the "base" (character set size estimate) for a password.
fn get_base(password: &str) -> i32 {
    let mut chars: Vec<char> = password.chars().collect();
    chars.sort();
    chars.dedup();

    let mut has_replace = false;
    let mut has_sep = false;
    let mut has_other_special = false;
    let mut has_lower = false;
    let mut has_upper = false;
    let mut has_digits = false;
    let mut base = 0;

    for c in chars {
        if REPLACE_CHARS.contains(c) {
            has_replace = true;
            continue;
        }
        if SEP_CHARS.contains(c) {
            has_sep = true;
            continue;
        }
        if OTHER_SPECIAL_CHARS.contains(c) {
            has_other_special = true;
            continue;
        }
        if LOWER_CHARS.contains(c) {
            has_lower = true;
            continue;
        }
        if UPPER_CHARS.contains(c) {
            has_upper = true;
            continue;
        }
        if DIGITS_CHARS.contains(c) {
            has_digits = true;
            continue;
        }
        base += 1;
    }

    if has_replace {
        base += REPLACE_CHARS.chars().count() as i32;
    }
    if has_sep {
        base += SEP_CHARS.chars().count() as i32;
    }
    if has_other_special {
        base += OTHER_SPECIAL_CHARS.chars().count() as i32;
    }
    if has_lower {
        base += LOWER_CHARS.chars().count() as i32;
    }
    if has_upper {
        base += UPPER_CHARS.chars().count() as i32;
    }
    if has_digits {
        base += DIGITS_CHARS.chars().count() as i32;
    }
    base
}

fn delete_rune_at(runes: &mut Vec<char>, i: usize) {
    if i >= runes.len() {
        return;
    }
    runes.remove(i);
}

fn remove_more_than_two_from_sequence(s: &str, seq: &str) -> String {
    let mut runes: Vec<char> = s.chars().collect();
    let seq_runes: Vec<char> = seq.chars().collect();
    let mut i = 0;
    let mut matches = 0;
    while i < runes.len() {
        let mut matched = false;
        for j in 0..seq_runes.len() {
            if i >= runes.len() {
                break;
            }
            let r = runes[i];
            let r2 = seq_runes[j];
            if r != r2 {
                matches = 0;
                continue;
            }
            matched = true;
            matches += 1;
            if matches > 2 {
                delete_rune_at(&mut runes, i);
            } else {
                i += 1;
            }
        }
        if !matched {
            i += 1;
            matches = 0;
        }
    }
    runes.into_iter().collect()
}

fn get_reversed_string(s: &str) -> String {
    s.chars().rev().collect()
}

fn remove_more_than_two_repeating_chars(s: &str) -> String {
    let mut runes: Vec<char> = s.chars().collect();
    let mut prev_prev: Option<char> = None;
    let mut prev: Option<char> = None;
    let mut i = 0;
    while i < runes.len() {
        let r = runes[i];
        if prev.is_some() && prev_prev.is_some() && r == prev.unwrap() && r == prev_prev.unwrap() {
            delete_rune_at(&mut runes, i);
            if i > 0 {
                i -= 1;
            }
            continue;
        }
        prev_prev = prev;
        prev = Some(r);
        i += 1;
    }
    runes.into_iter().collect()
}

fn get_length(password: &str) -> usize {
    let mut password = remove_more_than_two_repeating_chars(password);
    for seq in [
        SEQ_NUMS,
        SEQ_KEYBOARD0,
        SEQ_KEYBOARD1,
        SEQ_KEYBOARD2,
        SEQ_ALPHABET,
    ] {
        password = remove_more_than_two_from_sequence(&password, seq);
        password = remove_more_than_two_from_sequence(&password, &get_reversed_string(seq));
    }
    password.chars().count()
}

fn log_x(base: f64, n: f64) -> f64 {
    if base == 0.0 {
        return 0.0;
    }
    n.log2() / base.log2()
}

/// Calculate log(base, x^y) without overflowing.
fn log_pow(exp_base: f64, pow: usize, log_base: f64) -> f64 {
    let mut total = 0.0;
    for _ in 0..pow {
        total += log_x(log_base, exp_base);
    }
    total
}

/// Compute the password entropy in bits. Mirrors go-password-validator.
pub fn get_entropy(password: &str) -> f64 {
    let base = get_base(password);
    let length = get_length(password);
    log_pow(base as f64, length, 2.0)
}

/// Validate a password against a minimum entropy threshold.
/// Returns Ok(()) if entropy >= min_entropy_bits.
pub fn validate(password: &str, min_entropy_bits: f64) -> bool {
    get_entropy(password) >= min_entropy_bits
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entropy_matches_go() {
        // Values verified against Go's go-password-validator at min 30 bits.
        let cases = [
            ("password", 30, true),
            ("ComplexPass123!", 30, true),
            ("TestPass123!", 30, true),
            ("NewComplexPass456!", 30, true),
            ("abcd1234", 30, false),
            ("aaaaaaaa", 30, false),
            ("P@ssw0rd!", 30, true),
            ("abcdefgh", 30, false),
        ];
        for (pw, bits, expected) in cases {
            let entropy = get_entropy(pw);
            let pass = entropy >= bits as f64;
            assert_eq!(pass, expected, "password {:?} entropy={:.1} bits={}", pw, entropy, bits);
        }
    }

    #[test]
    fn test_entropy_25() {
        // Verified against Go: abcd1234 (20.68 bits) fails at 25 bits.
        assert!(!validate("abcd1234", 25.0));
        // password (37.6 bits) passes at 30.
        assert!(validate("password", 30.0));
    }
}

/// Debug helper: print intermediate length steps.
pub fn debug_length_steps(password: &str) -> String {
    let mut steps = Vec::new();
    let mut password = remove_more_than_two_repeating_chars(password);
    steps.push(format!("after repeat: {} (len {})", password, password.chars().count()));
    for seq in [
        SEQ_NUMS,
        SEQ_KEYBOARD0,
        SEQ_KEYBOARD1,
        SEQ_KEYBOARD2,
        SEQ_ALPHABET,
    ] {
        password = remove_more_than_two_from_sequence(&password, seq);
        steps.push(format!("after {}: {} (len {})", seq, password, password.chars().count()));
        password = remove_more_than_two_from_sequence(&password, &get_reversed_string(seq));
        steps.push(format!("after rev {}: {} (len {})", seq, password, password.chars().count()));
    }
    steps.join("\n")
}
