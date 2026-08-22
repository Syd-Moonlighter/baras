//! Semi-global alignment between an OCR reading and a log name.
//!
//! One DP pass replaces case-by-case prefix slicing. The error model:
//! a name's tail is often missing — clipped by the frame or occluded by UI
//! elements drawn over it — so the reading may stop anywhere inside the
//! target for free; marker icons leave junk after an otherwise
//! complete read, so the reading may run past a fully consumed target at a
//! discount; and a [`WILDCARD`] stands for a glyph OCR
//! detected but could not identify. Everything else — dropped glyphs,
//! hallucinated glyphs, misread letters — pays a unit edit cost, wherever in
//! the name it happens.

use super::MIN_OCR_NAME_CHARS;
use super::normalize::WILDCARD;

/// Aligning a wildcard against any letter. Presence is evidence, identity is
/// not, so it costs a fraction of a substitution instead of nothing.
const WILDCARD_SUB: f32 = 0.15;

/// An unmatched glyph at the very start of a reading. The crop's left edge
/// sometimes yields a stray letter (`ISOA` for `SOA`); cheaper than a full
/// insertion, but not free, so it cannot manufacture matches.
const LEADING_INSERT: f32 = 0.4;

/// A glyph left over after the whole target was consumed — marker icons
/// trailing an otherwise complete read. Cheap enough that real marker noise
/// survives, expensive enough that a roster name beats its own prefix
/// (`ALPHARHO` over `ALPHA`) by more than the ambiguity margin. Raising this
/// past ~0.35 would sink a three-glyph marker on a short name below the
/// confidence floor.
const JUNK_INSERT: f32 = 0.3;

/// Score in `0.0..=1.0` for `observed` being a reading of `target`.
///
/// Standard Levenshtein DP over the full strings, except that the score is
/// taken as the best over every cell of the last row (the target's unread
/// suffix is free: clipped or occluded) and, when the target is long enough to be
/// identity on its own, every cell of the last column (the reading's unused
/// suffix is discounted: trailing junk). Each end point is normalized by the length
/// actually consumed, so a charitable alignment cannot hide behind a short
/// one.
pub(super) fn align_score(observed: &str, target: &str) -> f32 {
    let a = observed.as_bytes();
    let b = target.as_bytes();
    let (m, n) = (a.len(), b.len());
    debug_assert!(m > 0 && n > 0, "callers guard empty inputs");

    let score = |cost: f32, i: usize, j: usize| 1.0 - cost / i.max(j).max(1) as f32;
    // A short target reached early would score any shared prefix as a full
    // match; only targets long enough to be identity get the free suffix.
    let junk_suffix_ok = n >= MIN_OCR_NAME_CHARS;

    let mut best = f32::MIN;
    let mut prev: Vec<f32> = (0..=n).map(|j| j as f32).collect();
    let mut curr = vec![0.0f32; n + 1];

    for (i, &ac) in a.iter().enumerate() {
        // First column: i + 1 leading insertions, the first one discounted.
        curr[0] = LEADING_INSERT + i as f32;
        for (j, &bc) in b.iter().enumerate() {
            let sub = if ac == bc {
                0.0
            } else if ac == WILDCARD {
                WILDCARD_SUB
            } else {
                1.0
            };
            curr[j + 1] = (prev[j + 1] + 1.0).min(curr[j] + 1.0).min(prev[j] + sub);
        }
        // Every glyph past the consumed target is charged, so two matched
        // letters plus a cheap deletion cannot claim a short name out of the
        // start of unrelated text.
        if junk_suffix_ok {
            let junk = (m - (i + 1)) as f32 * JUNK_INSERT;
            best = best.max(score(curr[n] + junk, i + 1, n));
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    for (j, &cost) in prev.iter().enumerate() {
        best = best.max(score(cost, m, j));
    }
    best.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipped_prefix_is_a_full_match() {
        assert_eq!(align_score("TESTCHARL", "TESTCHARLIELONG"), 1.0);
    }

    #[test]
    fn trailing_junk_is_cheap_but_not_free() {
        let score = align_score("ALPHARXO", "ALPHA");
        assert!(score > 0.75, "marker noise must survive, got {score}");
        assert!(score < 1.0, "junk is not a perfect read");
    }

    #[test]
    fn a_longer_name_beats_its_own_prefix_by_the_margin() {
        // Reading ALPHARHO with both ALPHA and ALPHARHO on the roster: the
        // exact match must lead the junk-suffix match by the ambiguity margin.
        let prefix = align_score("ALPHARHO", "ALPHA");
        assert!(1.0 - prefix > 0.10, "lead is only {}", 1.0 - prefix);
    }

    #[test]
    fn a_dropped_glyph_costs_one_omission() {
        // The old `+1` special case, now just one deletion in the alignment.
        let score = align_score("CAZOON", "CATZOON");
        assert!((score - (1.0 - 1.0 / 7.0)).abs() < 1e-6);
    }

    #[test]
    fn two_dropped_glyphs_cost_two_omissions() {
        // Beyond what prefix slicing could ever see.
        let score = align_score("CAZON", "CATZOON");
        assert!((score - (1.0 - 2.0 / 7.0)).abs() < 1e-6);
    }

    #[test]
    fn a_wildcard_is_nearly_free_against_any_letter() {
        let score = align_score("CA?ZOON", "CATZOON");
        assert!(score > 0.95);
        // ...and scores lookalikes identically, leaving them to the margin.
        assert_eq!(score, align_score("CA?ZOON", "CARZOON"));
    }

    #[test]
    fn a_wildcard_is_not_free_evidence() {
        assert!(align_score("CA?ZOON", "CATZOON") < 1.0);
    }

    #[test]
    fn unrelated_text_scores_low() {
        assert!(align_score("XQZRV", "ECHOFIVE") < 0.4);
    }

    #[test]
    fn short_target_does_not_get_the_free_junk_suffix() {
        // "AI" is below MIN_OCR_NAME_CHARS: matching its two letters must not
        // discard the rest of the reading for free.
        assert!(align_score("AIXXXX", "AI") < 0.5);
    }

    #[test]
    fn a_short_name_is_not_claimed_from_the_start_of_unrelated_text() {
        // NEWCOMER shares N and E with ONE. The junk-suffix rule must not let
        // one deletion plus two matches outweigh six unread characters.
        assert!(align_score("NEWCOMER", "ONE") < 0.5);
    }

    #[test]
    fn leading_artifact_is_discounted_but_not_free() {
        let score = align_score("ISOA", "SOA");
        assert!(score > 0.85 && score < 1.0);
    }
}
