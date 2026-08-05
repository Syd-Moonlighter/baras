//! Parse text recognized from a raid-frame band.

/// Parse the health text SWTOR prints in an ops frame.
///
/// Reads `271,245 (55%)` into `(Some(271245), Some(55))`. Either half may be
/// missing: the percentage is frequently covered by ops target markers, and the
/// numeric display is an option some users leave off.
///
/// Recognition routinely substitutes letters for digits at raid-frame sizes, so
/// the most common confusions are folded back before parsing.
pub fn parse_health_text(text: &str) -> (Option<u32>, Option<u8>) {
    let normalized: String = text
        .chars()
        .map(|c| match c {
            'O' | 'o' | 'D' | 'Q' => '0',
            'l' | 'I' | 'i' | '|' => '1',
            'S' => '5',
            'B' => '8',
            other => other,
        })
        .collect();

    // The percentage is whatever sits inside parentheses before a '%'.
    let percent = normalized
        .split(['(', ')'])
        .find_map(|chunk| chunk.trim().strip_suffix('%'))
        .and_then(|digits| digits.trim().parse::<u32>().ok())
        .filter(|p| *p <= 100)
        .map(|p| p as u8);

    // The absolute value is the digit run before the parenthesised part; commas
    // are thousands separators and are dropped.
    let before_paren = normalized.split('(').next().unwrap_or(&normalized);
    let digits: String = before_paren.chars().filter(char::is_ascii_digit).collect();
    let value = if digits.len() >= 3 {
        digits.parse::<u32>().ok()
    } else {
        None
    };

    (value, percent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_health_with_value_and_percent() {
        assert_eq!(parse_health_text("271,245 (55%)"), (Some(271_245), Some(55)));
        assert_eq!(parse_health_text("95,575 (22%)"), (Some(95_575), Some(22)));
        assert_eq!(
            parse_health_text("485,275 (100%)"),
            (Some(485_275), Some(100))
        );
    }

    #[test]
    fn parses_health_without_separators() {
        assert_eq!(
            parse_health_text("443604 (100%)"),
            (Some(443_604), Some(100))
        );
    }

    #[test]
    fn parses_value_when_percent_is_covered_by_a_marker() {
        assert_eq!(parse_health_text("498,994"), (Some(498_994), None));
        assert_eq!(parse_health_text("443,782 ("), (Some(443_782), None));
    }

    #[test]
    fn parses_percent_when_value_is_unreadable() {
        assert_eq!(parse_health_text("(76%)"), (None, Some(76)));
    }

    #[test]
    fn recovers_from_letter_for_digit_confusion() {
        assert_eq!(
            parse_health_text("44O,917 (1OO%)"),
            (Some(440_917), Some(100))
        );
        assert_eq!(parse_health_text("l15,319 (26%)"), (Some(115_319), Some(26)));
    }

    #[test]
    fn rejects_nonsense_rather_than_guessing() {
        assert_eq!(parse_health_text(""), (None, None));
        assert_eq!(parse_health_text("---"), (None, None));
        // Too few digits to be a health value.
        assert_eq!(parse_health_text("12"), (None, None));
        // Percentages above 100 are a misread, not a reading.
        assert_eq!(parse_health_text("(255%)"), (None, None));
    }

}
