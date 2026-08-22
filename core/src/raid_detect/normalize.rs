//! Normalize OCR and combat-log names to the same ASCII form.
//!
//! Separators are removed because SWTOR often loses them at raid-frame sizes.

/// ASCII fold for U+00C0..=U+00FF, indexed by `c - 0xC0`.
///
/// `×` and `÷` map to `x` but are stripped later by [`is_kept`]; they are only
/// present to keep the table dense.
///
/// `ß` (U+00DF) is deliberately left unfolded — see [`fold_char`].
const LATIN1_FOLD: &[u8; 64] = b"AAAAAAACEEEEIIIIDNOOOOOxOUUUUYP\0aaaaaaaceeeeiiiidnoooooxouuuuypy";

/// Fold a single character toward ASCII.
///
/// Returns the character unchanged when no fold applies; callers still have to
/// uppercase and filter afterwards.
fn fold_char(c: char) -> char {
    let cp = c as u32;

    // `ß` is left for `to_uppercase`, which expands it to "SS". SWTOR uppercases
    // it the same way on screen, so folding it to a single 's' here would make
    // the log name one character shorter than what OCR reads back.
    if c == 'ß' {
        return c;
    }

    // Latin-1 Supplement: dense enough for a table.
    if (0xC0..=0xFF).contains(&cp) {
        return LATIN1_FOLD[(cp - 0xC0) as usize] as char;
    }

    // Latin Extended-A: only the characters plausible in a SWTOR name. The full
    // block is 128 code points of mostly-unused letters, and a partial table
    // risks silent off-by-one errors, so these are spelled out.
    match c {
        'Ā' | 'Ă' | 'Ą' => 'A',
        'ā' | 'ă' | 'ą' => 'a',
        'Ć' | 'Ĉ' | 'Ċ' | 'Č' => 'C',
        'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'Ď' | 'Đ' => 'D',
        'ď' | 'đ' => 'd',
        'Ē' | 'Ĕ' | 'Ė' | 'Ę' | 'Ě' => 'E',
        'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'Ĝ' | 'Ğ' | 'Ġ' | 'Ģ' => 'G',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'Ĥ' | 'Ħ' => 'H',
        'ĥ' | 'ħ' => 'h',
        'Ĩ' | 'Ī' | 'Ĭ' | 'Į' | 'İ' => 'I',
        'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => 'i',
        'Ĵ' => 'J',
        'ĵ' => 'j',
        'Ķ' => 'K',
        'ķ' => 'k',
        'Ĺ' | 'Ļ' | 'Ľ' | 'Ł' => 'L',
        'ĺ' | 'ļ' | 'ľ' | 'ł' => 'l',
        'Ń' | 'Ņ' | 'Ň' => 'N',
        'ń' | 'ņ' | 'ň' => 'n',
        'Ō' | 'Ŏ' | 'Ő' => 'O',
        'ō' | 'ŏ' | 'ő' => 'o',
        'Ŕ' | 'Ŗ' | 'Ř' => 'R',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'Ś' | 'Ŝ' | 'Ş' | 'Š' => 'S',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'Ţ' | 'Ť' | 'Ŧ' => 'T',
        'ţ' | 'ť' | 'ŧ' => 't',
        'Ũ' | 'Ū' | 'Ŭ' | 'Ů' | 'Ű' | 'Ų' => 'U',
        'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'Ŵ' => 'W',
        'ŵ' => 'w',
        'Ŷ' | 'Ÿ' => 'Y',
        'ŷ' => 'y',
        'Ź' | 'Ż' | 'Ž' => 'Z',
        'ź' | 'ż' | 'ž' => 'z',
        _ => c,
    }
}

/// Characters that survive normalization.
///
/// Letters only. SWTOR names allow letters, one space, one hyphen and up to two
/// apostrophes — never digits. Any digit in a reading is therefore noise from an
/// icon at the edge of the crop, and dropping it tightens the comparison rather
/// than losing information.
fn is_kept(c: char) -> bool {
    c.is_ascii_alphabetic()
}

/// Strip combat-log decoration from a name.
///
/// Log entities are recorded as `@Test Alpha#101`; the leading
/// `@` marks a player and the `#` introduces a numeric id. Neither appears
/// on screen.
fn strip_log_decoration(name: &str) -> &str {
    let name = name.strip_prefix('@').unwrap_or(name);
    match name.find('#') {
        Some(idx) => &name[..idx],
        None => name,
    }
}

/// OCR's marker for a glyph it detected but could not identify. Kept by
/// [`normalize_ocr`] as a wildcard for the matcher; SWTOR names cannot contain
/// it, so [`normalize`]d log names never do.
pub const WILDCARD: u8 = b'?';

/// Normalize a name for comparison.
///
/// Applies, in order: log-decoration stripping, ASCII folding, uppercasing, and
/// removal of every character that is not a letter or digit. Spaces, apostrophes
/// and hyphens are dropped, so legacy surnames collapse into the given name.
///
/// ```ignore
/// assert_eq!(normalize("@Test Long-name#123"), "TESTLONGNAME");
/// assert_eq!(normalize("Test'Alpha"), "TESTALPHA");
/// assert_eq!(normalize("Tést Bravo"), "TESTBRAVO");
/// ```
pub fn normalize(name: &str) -> String {
    normalize_impl(name, false)
}

/// [`normalize`] for OCR readings: `?` survives as a [`WILDCARD`]. Dropping it
/// would erase the position of a glyph the OCR *did* detect, turning "one
/// unreadable letter" into "one letter short" for the matcher.
pub fn normalize_ocr(name: &str) -> String {
    normalize_impl(name, true)
}

/// Letters actually identified in a normalized reading — wildcards mark a
/// glyph's presence, not its identity, so they do not count toward minimums.
pub fn identified_len(normalized: &str) -> usize {
    normalized.bytes().filter(|&b| b != WILDCARD).count()
}

fn normalize_impl(name: &str, keep_wildcard: bool) -> String {
    let stripped = strip_log_decoration(name);
    let mut out = String::with_capacity(stripped.len());

    for c in stripped.chars() {
        if keep_wildcard && c == WILDCARD as char {
            out.push(c);
            continue;
        }
        let folded = fold_char(c);
        // `ß → s` and similar produce ASCII directly; uppercase after folding so
        // multi-char uppercase mappings cannot reintroduce non-ASCII.
        for upper in folded.to_uppercase() {
            if is_kept(upper) {
                out.push(upper);
            }
        }
    }

    out
}

/// Levenshtein distance between two ASCII strings.
///
/// Two rolling rows rather than a full matrix; names are short, but this runs
/// once per (row, candidate) pair and there can be 24 of each.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();

    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }

    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0usize; b.len() + 1];

    for (i, &ac) in a.iter().enumerate() {
        curr[0] = i + 1;
        for (j, &bc) in b.iter().enumerate() {
            let cost = usize::from(ac != bc);
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b.len()]
}

/// Similarity in `0.0..=1.0`, derived from edit distance over the longer string.
pub fn similarity(a: &str, b: &str) -> f32 {
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 1.0;
    }
    1.0 - (edit_distance(a, b) as f32 / longest as f32)
}
