/// Format a number as an ordinal string (1st, 2nd, 3rd, ...).
/// Mirrors Go's util.Ordinal. Chinese does not require ordinals.
pub fn ordinal(x: i32, lang: &str) -> String {
    let s = x.to_string();
    if lang.starts_with("zh") {
        return s;
    }
    let mut suffix = "th";
    match x % 10 {
        1 => {
            if x % 100 != 11 {
                suffix = "st";
            }
        }
        2 => {
            if x % 100 != 12 {
                suffix = "nd";
            }
        }
        3 => {
            if x % 100 != 13 {
                suffix = "rd";
            }
        }
        _ => {}
    }
    format!("{}{}", s, suffix)
}

/// Split a string into lines by "\r\n" or "\n".
pub fn split_lines(s: &str) -> Vec<String> {
    let sep = if s.contains("\r\n") { "\r\n" } else { "\n" };
    s.split(sep).map(|l| l.to_string()).collect()
}

/// Go-style WriteString: concatenate strings.
pub fn write_string(strs: &[&str]) -> String {
    strs.concat()
}

/// Normalize a URL with an https scheme to just its hostname.
pub fn to_hostname(url: &str) -> String {
    let stripped = url.strip_prefix("https://").unwrap_or(url);
    stripped.split('/').next().unwrap_or("").to_string()
}

/// RFC3986 percent-encoding with `+` -> `%20`, `*` -> `%2A`, `%7E` -> `~`.
/// Mirrors Go's util.PercentEncode.
pub fn percent_encode(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let encoded = percent_encoding::utf8_percent_encode(value, percent_encoding::NON_ALPHANUMERIC).to_string();
    encoded
        .replace('+', "%20")
        .replace('*', "%2A")
        .replace("%7E", "~")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ordinal() {
        // Mirrors Go's TestOrdinal
        let cases = [
            (0, "0th"),
            (1, "1st"),
            (2, "2nd"),
            (3, "3rd"),
            (4, "4th"),
            (10, "10th"),
            (11, "11th"),
            (12, "12th"),
            (13, "13th"),
            (21, "21st"),
            (32, "32nd"),
            (43, "43rd"),
            (101, "101st"),
            (102, "102nd"),
            (103, "103rd"),
            (211, "211th"),
            (212, "212th"),
            (213, "213th"),
        ];
        for (input, expected) in cases {
            assert_eq!(ordinal(input, "en"), expected, "ordinal({}, en)", input);
        }
        // Chinese returns plain number
        assert_eq!(ordinal(1, "zh"), "1");
    }

    #[test]
    fn test_write_string() {
        assert_eq!(write_string(&["hello", "world"]), "helloworld");
        assert_eq!(write_string(&["", "test"]), "test");
        assert_eq!(write_string(&["hello", " ", "world"]), "hello world");
        assert_eq!(write_string(&[""]), "");
    }

    #[test]
    fn test_to_hostname() {
        // Mirrors Go's TestToHostname
        assert_eq!(to_hostname("https://www.example.com"), "www.example.com");
        assert_eq!(to_hostname("www.example.com/path"), "www.example.com");
        assert_eq!(to_hostname("https://www.example.com/path"), "www.example.com");
    }

    #[test]
    fn test_split_lines() {
        assert_eq!(split_lines("a\nb\nc"), vec!["a", "b", "c"]);
        assert_eq!(split_lines("a\r\nb"), vec!["a", "b"]);
    }

    #[test]
    fn test_percent_encode() {
        // Mirrors Go's util.PercentEncode
        assert_eq!(percent_encode("hello world"), "hello%20world");
        assert_eq!(percent_encode("a*b"), "a%2Ab");
        assert_eq!(percent_encode("a~b"), "a~b");
        assert_eq!(percent_encode(""), "");
    }
}
