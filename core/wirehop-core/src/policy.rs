//! Receiver-side validation of untrusted transfer metadata.
//!
//! From `docs/references/PROTOCOL.md` §"Receiver validation of metadata".
//! Every value here crosses a trust boundary. Skipping these checks is unsafe,
//! not merely lenient: `filename` in particular is a bare filename and must
//! never be allowed to act as a path.

/// Maximum files one transfer may declare.
pub const MAX_FILES_PER_TRANSFER: usize = 1024;
/// Maximum filename length, in UTF-8 bytes.
pub const MAX_FILENAME_BYTES: usize = 255;
/// Maximum device name length, in UTF-8 bytes.
pub const MAX_DEVICE_NAME_BYTES: usize = 255;
/// Maximum size of any single file: 1 TiB.
pub const MAX_FILE_SIZE: u64 = 1024 * 1024 * 1024 * 1024;
/// Maximum total size of one transfer: 4 TiB.
pub const MAX_TOTAL_SIZE: u64 = MAX_FILE_SIZE * 4;

const FORBIDDEN_CHARS: &[char] = &['<', '>', ':', '"', '|', '?', '*'];

/// Rejects Unicode control and bidirectional-override characters, which can
/// make a displayed filename differ from the bytes written to disk.
fn contains_unsafe_unicode(text: &str) -> bool {
    text.chars().any(|c| {
        c.is_control()
            || matches!(c,
                '\u{200E}' | '\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}')
    })
}

/// Windows reserved device names, which are illegal regardless of extension.
fn is_reserved_windows_name(filename: &str) -> bool {
    let stem = filename.split('.').next().unwrap_or(filename);
    let upper = stem.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL") {
        return true;
    }
    if let Some(rest) = upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
    {
        return matches!(rest, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9");
    }
    false
}

/// Whether an incoming filename may be written to the destination directory.
pub fn is_safe_filename(filename: &str) -> bool {
    if filename.is_empty() || filename == "." || filename == ".." {
        return false;
    }
    if filename.len() > MAX_FILENAME_BYTES {
        return false;
    }
    if filename.contains('/') || filename.contains('\\') {
        return false;
    }
    // A Windows drive prefix would make this absolute on that platform even
    // without a separator.
    if filename.len() >= 2 && filename.as_bytes()[1] == b':' {
        return false;
    }
    if filename.ends_with('.') || filename.ends_with(' ') {
        return false;
    }
    if contains_unsafe_unicode(filename) {
        return false;
    }
    if filename.contains(FORBIDDEN_CHARS) {
        return false;
    }
    !is_reserved_windows_name(filename)
}

/// Whether a peer-supplied device name may be displayed.
pub fn is_safe_device_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= MAX_DEVICE_NAME_BYTES && !contains_unsafe_unicode(name)
}

/// Converts a JSON-sourced size to an exact byte count.
///
/// JSON numbers are doubles, so a value must be finite, non-negative, integral,
/// and within range. Anything else is a protocol error rather than a clamp.
pub fn parse_file_size(value: f64) -> Option<u64> {
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > MAX_FILE_SIZE as f64 {
        return None;
    }
    Some(value as u64)
}

/// Whether another file of `size` still fits within the transfer ceiling.
pub fn can_append_file(current_total: u64, size: u64) -> bool {
    size <= MAX_FILE_SIZE
        && current_total <= MAX_TOTAL_SIZE
        && current_total.saturating_add(size) <= MAX_TOTAL_SIZE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_portable_filenames() {
        for name in ["a.txt", "photo 01.jpeg", "报告.pdf", "no-ext"] {
            assert!(is_safe_filename(name), "{name} should be accepted");
        }
    }

    #[test]
    fn rejects_unsafe_filenames() {
        let cases = [
            ("", "empty"),
            (".", "dot"),
            ("..", "dot-dot"),
            ("../etc/passwd", "relative traversal"),
            ("a/b.txt", "forward slash"),
            ("a\\b.txt", "backslash"),
            ("C:file.txt", "drive prefix"),
            ("trailing.", "trailing dot"),
            ("trailing ", "trailing space"),
            ("bad<name>.txt", "forbidden character"),
            ("CON", "reserved name"),
            ("com1.txt", "reserved name with extension"),
            ("bell\u{7}.txt", "control character"),
            ("rtl\u{202E}gpj.txt", "bidi override"),
        ];
        for (name, why) in cases {
            assert!(
                !is_safe_filename(name),
                "{why:?} should be rejected: {name:?}"
            );
        }
    }

    #[test]
    fn bounds_filename_in_utf8_bytes() {
        assert!(is_safe_filename(&"a".repeat(MAX_FILENAME_BYTES)));
        assert!(!is_safe_filename(&"a".repeat(MAX_FILENAME_BYTES + 1)));

        // 100 CJK characters are 300 UTF-8 bytes, over the bound.
        assert!(!is_safe_filename(&"中".repeat(100)));
    }

    #[test]
    fn validates_device_names() {
        assert!(is_safe_device_name("MacBook"));
        assert!(!is_safe_device_name(""));
        assert!(!is_safe_device_name(&"a".repeat(MAX_DEVICE_NAME_BYTES + 1)));
        assert!(!is_safe_device_name("evil\u{202E}name"));
    }

    #[test]
    fn validates_file_sizes() {
        assert_eq!(parse_file_size(0.0), Some(0));
        assert_eq!(parse_file_size(1234.0), Some(1234));
        assert_eq!(parse_file_size(MAX_FILE_SIZE as f64), Some(MAX_FILE_SIZE));

        assert_eq!(parse_file_size(-1.0), None);
        assert_eq!(parse_file_size(1.5), None);
        assert_eq!(parse_file_size(f64::NAN), None);
        assert_eq!(parse_file_size(f64::INFINITY), None);
        assert_eq!(parse_file_size(MAX_FILE_SIZE as f64 * 2.0), None);
    }

    #[test]
    fn enforces_the_total_transfer_ceiling() {
        assert!(can_append_file(0, MAX_FILE_SIZE));
        assert!(can_append_file(MAX_TOTAL_SIZE - 1, 1));
        assert!(!can_append_file(MAX_TOTAL_SIZE, 1));
        assert!(!can_append_file(MAX_TOTAL_SIZE - 1, 2));
    }
}
