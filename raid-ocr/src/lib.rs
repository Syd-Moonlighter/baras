//! OCR orchestration for SWTOR raid frames.
//!
//! This is separate from Tauri so recorded frames can run through the same
//! pipeline as live captures. Unreadable rows stay unassigned.

pub mod engine;

use baras_core::raid_detect::{
    MIN_OCR_NAME_CHARS, MatchConfig, PlayerCandidate, RowAssignment, RowObservation,
};
use baras_overlay::capture::CapturedImage;
use baras_overlay::capture::analysis::{
    BandKind, detect_bands, harmonize, parse_health_text, prepare,
};

/// A slot rectangle within the captured overlay image.
pub type SlotRect = (u8, i32, i32, u32, u32);

/// Usable slot labels when there is no combat-log roster to match against.
pub fn ocr_only_names(observations: &[RowObservation]) -> Vec<(u8, String)> {
    observations
        .iter()
        .filter_map(|row| {
            let name = clean_for_display(row.name_text.as_deref()?);
            if baras_core::raid_detect::normalize(&name).len() < MIN_OCR_NAME_CHARS {
                return None;
            }
            Some((u8::try_from(row.row).ok()?, name))
        })
        .collect()
}

/// Tidy a raw reading for display.
///
/// Provisional names are shown to the user verbatim, so the stray glyph the
/// crop's left edge picks up — `[VENNDRICK`, `|AZURIS` — reaches the screen and
/// reads as a bug even though matching discards it. SWTOR names contain only
/// letters, one space, one hyphen and up to two apostrophes, so anything else at
/// either end cannot belong to a real name and is safe to drop.
///
/// Only the ends are touched. A reading is still a guess, and rewriting the
/// middle would make a wrong guess harder to recognise as wrong.
fn clean_for_display(text: &str) -> String {
    let trimmed = text.trim_matches(|c: char| !c.is_alphabetic());

    // The other shape this artifact takes is a lone leading letter before a
    // space (`I CHIE SIA`). SWTOR requires at least three characters either side
    // of a space, so a one- or two-letter first part cannot be genuine.
    match trimmed.split_once(' ') {
        Some((first, rest)) if first.chars().count() < 3 && !rest.trim().is_empty() => {
            rest.trim().to_string()
        }
        _ => trimmed.to_string(),
    }
}

/// Read every slot in a captured overlay region.
///
/// Empty slots are omitted.
pub fn observe_slots(image: &CapturedImage, slots: &[SlotRect]) -> Vec<RowObservation> {
    // Find every band first so outliers can be corrected across the grid.
    let mut slot_images = Vec::with_capacity(slots.len());
    let mut per_slot_bands = Vec::with_capacity(slots.len());

    for &(_, x, y, w, h) in slots {
        match image.crop(x, y, w, h) {
            Some(slot_image) => {
                per_slot_bands.push(detect_bands(&slot_image));
                slot_images.push(Some(slot_image));
            }
            None => {
                per_slot_bands.push(Vec::new());
                slot_images.push(None);
            }
        }
    }

    harmonize(&mut per_slot_bands);

    let mut observations = Vec::new();

    for ((slot_rect, slot_image), slot_bands) in slots.iter().zip(&slot_images).zip(&per_slot_bands)
    {
        let Some(slot_image) = slot_image else {
            continue;
        };

        let mut observation = RowObservation {
            row: slot_rect.0 as usize,
            ..Default::default()
        };
        let mut saw_anything = false;

        for band in slot_bands {
            let Some(crop) = prepare(slot_image, band) else {
                continue;
            };
            let text = match engine::recognize(&crop) {
                Ok(text) if !text.trim().is_empty() => text,
                Ok(_) => continue,
                Err(e) => {
                    tracing::debug!("Slot {} band {:?} unreadable: {e}", slot_rect.0, band.kind);
                    continue;
                }
            };

            match band.kind {
                BandKind::Name => {
                    observation.name_text = Some(text);
                    saw_anything = true;
                }
                BandKind::Health => {
                    let (value, percent) = parse_health_text(&text);
                    observation.hp_value = value;
                    observation.hp_percent = percent;
                    saw_anything |= value.is_some() || percent.is_some();
                }
            }
        }

        if saw_anything {
            observations.push(observation);
        }
    }

    observations
}

/// Full detection pass: read the screen, then match against known players.
pub fn detect(
    image: &CapturedImage,
    slots: &[SlotRect],
    candidates: &[PlayerCandidate],
) -> Vec<RowAssignment> {
    let observations = observe_slots(image, slots);
    tracing::debug!(
        "Raid detection read {} of {} slots against {} candidates",
        observations.len(),
        slots.len(),
        candidates.len()
    );
    baras_core::raid_detect::assign_rows(&observations, candidates, &MatchConfig::default())
}

#[cfg(test)]
mod display_tests {
    use super::clean_for_display;

    #[test]
    fn strips_the_left_edge_artifact() {
        // Observed live: the crop's left edge yields a stray glyph.
        assert_eq!(clean_for_display("[VENNDRICK"), "VENNDRICK");
        assert_eq!(clean_for_display("|AZURIS"), "AZURIS");
        assert_eq!(clean_for_display("|MORE ORDER"), "MORE ORDER");
        assert_eq!(clean_for_display("[VENNDRICK $"), "VENNDRICK");
    }

    #[test]
    fn strips_a_lone_leading_letter_before_a_space() {
        // SWTOR requires three characters either side of a space, so a one-letter
        // first part is an artifact rather than a name part.
        assert_eq!(clean_for_display("I CHIE SIA"), "CHIE SIA");
        assert_eq!(clean_for_display("ISOA"), "ISOA", "no space, leave it alone");
    }

    #[test]
    fn leaves_legitimate_names_untouched() {
        assert_eq!(clean_for_display("FRYKANYSUYA"), "FRYKANYSUYA");
        assert_eq!(clean_for_display("MORE ORDER"), "MORE ORDER");
        assert_eq!(clean_for_display("Test Long-name"), "Test Long-name");
        // Interior punctuation is part of the name and must survive.
        assert_eq!(clean_for_display("T'EST ALPHA"), "T'EST ALPHA");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_only_mode_keeps_names_without_inventing_entities() {
        let observations = vec![
            RowObservation {
                row: 2,
                name_text: Some("  PLAYER 8K  ".into()),
                hp_value: Some(493_172),
                hp_percent: Some(100),
            },
            RowObservation {
                row: 3,
                name_text: Some("DS".into()),
                ..Default::default()
            },
            RowObservation {
                row: 4,
                name_text: Some("BOT".into()),
                ..Default::default()
            },
            RowObservation {
                row: 300,
                name_text: Some("OUT OF RANGE".into()),
                ..Default::default()
            },
        ];

        assert_eq!(
            ocr_only_names(&observations),
            vec![(2, "PLAYER 8K".into()), (4, "BOT".into())]
        );
    }
}
