//! Making untrusted text safe to display.

use std::borrow::Cow;

/// Replaces control characters (ESC, BEL, CR, ...) with U+FFFD, so text that came
/// from the OS (volume, interface or sensor names) can't inject terminal sequences.
pub fn printable(text: &str) -> Cow<'_, str> {
    if text.chars().any(char::is_control) {
        Cow::Owned(
            text.chars()
                .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
                .collect(),
        )
    } else {
        Cow::Borrowed(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_control_characters() {
        assert_eq!(printable("Macintosh HD"), "Macintosh HD");
        assert!(matches!(printable("plain"), Cow::Borrowed(_)));
        let evil = printable("\x1b]52;c;cm0=\x07X\x1b[2J\r\n");
        assert!(!evil.chars().any(char::is_control), "{evil:?}");
        assert!(evil.contains('X'));
    }
}
