//! Locate name and health bands inside a raid-frame slot.
//!
//! Names are light-on-dark; health text sits on a red bar. Band positions are
//! reconciled across slots because markers can throw off one row profile.

use crate::capture::CapturedImage;

/// A horizontal strip within a slot believed to contain text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Band {
    /// Offset from the top of the slot, in pixels.
    pub top: u32,
    /// Height of the strip, in pixels.
    pub height: u32,
    pub kind: BandKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BandKind {
    /// Light text on the dark frame background.
    Name,
    /// Light text on the red health bar.
    Health,
}

/// Rows whose ink coverage is below this fraction hold no text worth reading.
const MIN_INK_FRACTION: f32 = 0.02;
/// Rows above this are a solid fill (a full health bar), not glyphs.
const MAX_INK_FRACTION: f32 = 0.75;
/// Bands thinner than this cannot hold a legible glyph even after upscaling.
const MIN_BAND_HEIGHT: u32 = 4;
/// Ignore the right edge when locating names. Raid markers and buff icons live there.
pub(super) const NAME_SCAN_FRACTION: f32 = 0.75;
/// A pixel counts as "red bar" past this much red dominance.
const RED_DOMINANCE: i32 = 30;
/// Broad red runs include markers; their dense core is the actual health bar.
const MIN_HEALTH_RED_FRACTION: f32 = 0.25;
const HEALTH_CORE_PEAK_FRACTION: f32 = 0.55;
/// Offset from the slot median. Fixed thresholds miss dimmed frames.
const INK_ABOVE_MEDIAN: u16 = 35;
/// Floor for the ink threshold, so a nearly black slot does not treat noise as text.
const MIN_INK_LUMA: u8 = 60;

fn luma(r: u8, g: u8, b: u8) -> u8 {
    // Integer approximation of Rec. 601 luma; exactness is irrelevant here, the
    // threshold is empirical either way.
    (((r as u32 * 77) + (g as u32 * 150) + (b as u32 * 29)) >> 8) as u8
}

fn is_red_bar(r: u8, g: u8, b: u8) -> bool {
    (r as i32 - g as i32) > RED_DOMINANCE && (r as i32 - b as i32) > RED_DOMINANCE
}

/// Per-row statistics used to decide where text lives.
struct RowProfile {
    /// Fraction of pixels bright enough to be glyph ink.
    ink: f32,
    /// Fraction of pixels that look like the red health bar.
    red: f32,
}

/// Ink threshold for this slot, derived from its own brightness distribution.
fn ink_threshold(slot: &CapturedImage) -> u8 {
    // A histogram avoids sorting every pixel.
    let mut histogram = [0u32; 256];
    for px in slot.rgba.chunks_exact(4) {
        histogram[luma(px[0], px[1], px[2]) as usize] += 1;
    }

    let total: u32 = histogram.iter().sum();
    if total == 0 {
        return MIN_INK_LUMA;
    }

    let mut seen = 0u32;
    let mut median = 0u8;
    for (value, count) in histogram.iter().enumerate() {
        seen += count;
        if seen * 2 >= total {
            median = value as u8;
            break;
        }
    }

    (((median as u16 + INK_ABOVE_MEDIAN).min(255)) as u8).max(MIN_INK_LUMA)
}

fn profile_rows(slot: &CapturedImage) -> Vec<RowProfile> {
    let mut rows = Vec::with_capacity(slot.height as usize);
    let ink_luma = ink_threshold(slot);
    let name_width =
        ((slot.width as f32 * NAME_SCAN_FRACTION).round() as u32).clamp(1, slot.width.max(1));

    for y in 0..slot.height {
        let mut ink = 0u32;
        let mut red = 0u32;

        for x in 0..slot.width {
            let Some((r, g, b, _)) = slot.pixel(x, y) else {
                continue;
            };
            if is_red_bar(r, g, b) {
                red += 1;
            } else if x < name_width && luma(r, g, b) >= ink_luma {
                ink += 1;
            }
        }

        rows.push(RowProfile {
            ink: ink as f32 / name_width as f32,
            red: red as f32 / slot.width.max(1) as f32,
        });
    }

    rows
}

/// Per-row `(ink, red)` fractions, for diagnosing why a slot did or did not
/// yield bands. Not used by detection itself.
pub fn row_fractions(slot: &CapturedImage) -> Vec<(f32, f32)> {
    profile_rows(slot).into_iter().map(|p| (p.ink, p.red)).collect()
}

/// Find at most one name band and one health band.
pub fn detect_bands(slot: &CapturedImage) -> Vec<Band> {
    if slot.height < MIN_BAND_HEIGHT || slot.width == 0 {
        return Vec::new();
    }

    let rows = profile_rows(slot);
    let mut bands = Vec::new();

    if let Some(band) = health_band(&rows) {
        bands.push(Band {
            top: band.0,
            height: band.1,
            kind: BandKind::Health,
        });
    }

    // Names sit directly above health. Starting there avoids taller runs made by
    // role icons and buffs at the top of the frame.
    let name_rows = bands
        .first()
        .map_or(rows.as_slice(), |health| &rows[..health.top as usize]);
    let name_band = if bands.is_empty() {
        longest_run(name_rows, |p| {
            p.ink > MIN_INK_FRACTION && p.ink < MAX_INK_FRACTION && p.red < 0.10
        })
        .filter(|(_, len)| *len >= MIN_BAND_HEIGHT)
    } else {
        last_run_at_least(
            name_rows,
            |p| p.ink > MIN_INK_FRACTION && p.ink < MAX_INK_FRACTION,
            MIN_BAND_HEIGHT,
        )
    };

    if let Some(band) = name_band {
        bands.push(Band {
            top: band.0,
            height: band.1,
            kind: BandKind::Name,
        });
    }

    bands
}

fn health_band(rows: &[RowProfile]) -> Option<(u32, u32)> {
    let broad = longest_run(rows, |p| p.red > MIN_HEALTH_RED_FRACTION)
        .filter(|(_, len)| *len >= MIN_BAND_HEIGHT)?;
    let broad_rows = &rows[broad.0 as usize..(broad.0 + broad.1) as usize];
    let peak = broad_rows.iter().map(|p| p.red).fold(0.0f32, f32::max);
    let core_floor = (peak * HEALTH_CORE_PEAK_FRACTION).max(MIN_HEALTH_RED_FRACTION);

    longest_run(broad_rows, |p| p.red >= core_floor)
        .filter(|(_, len)| *len >= MIN_BAND_HEIGHT)
        .map(|(top, height)| (broad.0 + top, height))
        .or(Some(broad))
}

/// Longest consecutive run of rows satisfying `pred`, as `(start, length)`.
fn longest_run(rows: &[RowProfile], pred: impl Fn(&RowProfile) -> bool) -> Option<(u32, u32)> {
    let mut best: Option<(u32, u32)> = None;
    let mut start: Option<u32> = None;

    for (i, row) in rows.iter().enumerate() {
        let i = i as u32;
        if pred(row) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take() {
            let len = i - s;
            if best.is_none_or(|(_, bl)| len > bl) {
                best = Some((s, len));
            }
        }
    }

    if let Some(s) = start {
        let len = rows.len() as u32 - s;
        if best.is_none_or(|(_, bl)| len > bl) {
            best = Some((s, len));
        }
    }

    best
}

fn last_run_at_least(
    rows: &[RowProfile],
    pred: impl Fn(&RowProfile) -> bool,
    min_height: u32,
) -> Option<(u32, u32)> {
    let mut end = None;

    for (i, row) in rows.iter().enumerate().rev() {
        if pred(row) {
            end.get_or_insert(i as u32 + 1);
        } else if let Some(end) = end.take() {
            let top = i as u32 + 1;
            if end - top >= min_height {
                return Some((top, end - top));
            }
        }
    }

    end.and_then(|end| (end >= min_height).then_some((0, end)))
}

/// Pull clear vertical outliers back to the grid median.
/// Fewer readable slots than this and there is no consensus worth trusting.
const MIN_CONSENSUS_SLOTS: usize = 3;

pub fn harmonize(per_slot: &mut [Vec<Band>]) {
    for kind in [BandKind::Name, BandKind::Health] {
        // Bottoms, not tops: icons above the text stretch a band upward, never
        // down, so the bottom is the edge the slots agree on.
        let found: Vec<(u32, u32)> = per_slot
            .iter()
            .filter_map(|bands| {
                bands
                    .iter()
                    .find(|b| b.kind == kind)
                    .map(|b| (b.height, b.top + b.height))
            })
            .collect();

        if found.len() < MIN_CONSENSUS_SLOTS {
            continue;
        }

        let heights: Vec<u32> = found.iter().map(|(height, _)| *height).collect();
        let bottoms: Vec<u32> = found.iter().map(|(_, bottom)| *bottom).collect();
        let consensus_height = median(&heights);
        let consensus_bottom = median(&bottoms);
        // Scales with the frame size.
        let tolerance = (consensus_height * 2 / 5).max(2);

        if drifting(&bottoms, tolerance) {
            continue;
        }

        for bands in per_slot.iter_mut() {
            if let Some(band) = bands.iter_mut().find(|b| b.kind == kind) {
                let bottom = band.top + band.height;
                let too_tall = band.height.abs_diff(consensus_height) > tolerance;
                let displaced = bottom.abs_diff(consensus_bottom) > tolerance;

                // Small variation is row-profile noise and the padding absorbs
                // it. A band that only grew upward keeps its own bottom edge.
                if too_tall || displaced {
                    let anchor = if displaced { consensus_bottom } else { bottom };
                    band.top = anchor.saturating_sub(consensus_height);
                    band.height = consensus_height;
                }
            }
        }

        // A slot holding the other band has a player in it, so a missing band is
        // a detection failure rather than an empty frame. The health bar stops
        // being red enough to find below about a quarter health, which is where
        // the number matters most.
        for bands in per_slot.iter_mut() {
            if bands.is_empty() || bands.iter().any(|b| b.kind == kind) {
                continue;
            }
            bands.push(Band {
                top: consensus_bottom.saturating_sub(consensus_height),
                height: consensus_height,
                kind,
            });
        }
    }
}

/// Whether the bottoms trend one way down the slots instead of scattering.
///
/// That is the overlay grid not matching the game's, so the bands really do sit
/// at different offsets and a consensus would make every slot equally wrong.
fn drifting(bottoms: &[u32], tolerance: u32) -> bool {
    let (Some(min), Some(max)) = (bottoms.iter().min(), bottoms.iter().max()) else {
        return false;
    };
    if max - min <= tolerance {
        return false;
    }
    // Strictly, so one slot jumping while the rest agree stays an outlier.
    bottoms.windows(2).all(|w| w[0] < w[1]) || bottoms.windows(2).all(|w| w[0] > w[1])
}

/// Lower middle on an even count, so tall bands cannot drag the consensus up
/// toward themselves.
fn median(values: &[u32]) -> u32 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    sorted[(sorted.len() - 1) / 2]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a slot image: dark panel, a light text row, and a red bar with
    /// light glyphs — roughly how SWTOR draws an ops frame.
    fn synthetic_slot() -> CapturedImage {
        let (w, h) = (100u32, 40u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];

        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                let (r, g, b) = if (10..16).contains(&y) {
                    // Name text: light glyphs on dark, roughly 30% coverage.
                    if x % 3 == 0 {
                        (220, 225, 235)
                    } else {
                        (30, 40, 60)
                    }
                } else if (22..32).contains(&y) {
                    // Health bar: saturated red, with lighter digits on top.
                    if x % 4 == 0 {
                        (240, 240, 240)
                    } else {
                        (170, 30, 30)
                    }
                } else {
                    (30, 40, 60)
                };
                rgba[i] = r;
                rgba[i + 1] = g;
                rgba[i + 2] = b;
                rgba[i + 3] = 255;
            }
        }

        CapturedImage {
            width: w,
            height: h,
            rgba,
        }
    }

    fn paint(
        image: &mut CapturedImage,
        x: std::ops::Range<u32>,
        y: std::ops::Range<u32>,
        rgb: (u8, u8, u8),
    ) {
        for y in y {
            for x in x.clone() {
                let i = ((y * image.width + x) * 4) as usize;
                image.rgba[i..i + 4].copy_from_slice(&[rgb.0, rgb.1, rgb.2, 255]);
            }
        }
    }

    #[test]
    fn finds_both_bands() {
        let bands = detect_bands(&synthetic_slot());

        let health = bands.iter().find(|b| b.kind == BandKind::Health);
        let name = bands.iter().find(|b| b.kind == BandKind::Name);

        let health = health.expect("health bar should be found");
        assert!(
            (22..=23).contains(&health.top),
            "health band started at {}",
            health.top
        );

        let name = name.expect("name row should be found");
        assert!(
            (10..=11).contains(&name.top),
            "name band started at {}",
            name.top
        );
    }

    #[test]
    fn red_marker_does_not_swallow_name_or_health() {
        let mut slot = synthetic_slot();
        paint(&mut slot, 60..95, 2..40, (220, 20, 20));

        let bands = detect_bands(&slot);
        let health = bands
            .iter()
            .find(|band| band.kind == BandKind::Health)
            .expect("health bar should survive the marker");
        let name = bands
            .iter()
            .find(|band| band.kind == BandKind::Name)
            .expect("name should survive the marker");

        assert_eq!((health.top, health.height), (22, 10));
        assert_eq!((name.top, name.height), (10, 6));
    }

    #[test]
    fn name_nearest_health_wins_over_icons() {
        let mut slot = synthetic_slot();
        paint(&mut slot, 0..40, 1..8, (225, 230, 240));

        let bands = detect_bands(&slot);
        let name = bands
            .iter()
            .find(|band| band.kind == BandKind::Name)
            .expect("name should be found");

        assert_eq!((name.top, name.height), (10, 6));
    }

    #[test]
    fn empty_slot_yields_nothing() {
        let flat = CapturedImage {
            width: 50,
            height: 30,
            rgba: vec![20u8; 50 * 30 * 4],
        };
        assert!(detect_bands(&flat).is_empty());
    }

    #[test]
    fn tiny_slot_is_rejected() {
        let tiny = CapturedImage {
            width: 10,
            height: 2,
            rgba: vec![255u8; 10 * 2 * 4],
        };
        assert!(detect_bands(&tiny).is_empty());
    }

    #[test]
    fn harmonize_pulls_outlier_to_consensus() {
        let good = vec![Band {
            top: 10,
            height: 6,
            kind: BandKind::Name,
        }];
        let outlier = vec![Band {
            top: 30,
            height: 6,
            kind: BandKind::Name,
        }];

        let mut slots = vec![good.clone(), good.clone(), good.clone(), outlier];
        harmonize(&mut slots);

        assert_eq!(slots[3][0].top, 10, "outlier should snap to the consensus");
        assert_eq!(slots[0][0].top, 10, "agreeing slots must be left alone");
    }

    #[test]
    fn harmonize_leaves_minor_variation_alone() {
        let mut slots = vec![
            vec![Band {
                top: 10,
                height: 6,
                kind: BandKind::Name,
            }],
            vec![Band {
                top: 11,
                height: 6,
                kind: BandKind::Name,
            }],
            vec![Band {
                top: 12,
                height: 6,
                kind: BandKind::Name,
            }],
        ];
        harmonize(&mut slots);

        assert_eq!(slots[1][0].top, 11);
        assert_eq!(slots[2][0].top, 12);
    }

    /// Bands measured off a live 8-man frame. Slots 4 and 7 swallowed the icons
    /// above the text: tops ten pixels high, bottoms unchanged.
    #[test]
    fn harmonize_trims_a_band_that_only_grew_upward() {
        let observed = [
            (13, 9),
            (10, 13),
            (13, 10),
            (13, 10),
            (3, 19),
            (10, 13),
            (12, 11),
            (4, 19),
        ];
        let mut slots: Vec<Vec<Band>> = observed
            .iter()
            .map(|&(top, height)| {
                vec![Band {
                    top,
                    height,
                    kind: BandKind::Name,
                }]
            })
            .collect();

        harmonize(&mut slots);

        for (slot, band) in slots.iter().enumerate() {
            let band = band[0];
            assert!(
                band.height <= 13,
                "slot {slot} kept an over-tall band: {band:?}"
            );
            assert_eq!(
                band.top + band.height,
                observed[slot].0 + observed[slot].1,
                "slot {slot} must keep its own bottom edge"
            );
        }
        // The slots that agreed are untouched.
        assert_eq!((slots[0][0].top, slots[0][0].height), (13, 9));
        assert_eq!((slots[6][0].top, slots[6][0].height), (12, 11));
    }

    /// Bands creeping one way are a misaligned overlay, not misdetection.
    #[test]
    fn harmonize_leaves_a_drifting_grid_alone() {
        let mut slots: Vec<Vec<Band>> = [3, 6, 9, 12, 15, 18]
            .iter()
            .map(|&top| {
                vec![Band {
                    top,
                    height: 6,
                    kind: BandKind::Name,
                }]
            })
            .collect();

        harmonize(&mut slots);

        assert_eq!(slots[0][0].top, 3);
        assert_eq!(slots[5][0].top, 18);
    }

    /// A low health bar carries too little red to be found, so the slot inherits
    /// the band the others agree on. An empty frame still gets nothing.
    #[test]
    fn harmonize_fills_in_a_band_the_slot_could_not_find() {
        let full = |top| {
            vec![
                Band {
                    top,
                    height: 18,
                    kind: BandKind::Health,
                },
                Band {
                    top: 10,
                    height: 13,
                    kind: BandKind::Name,
                },
            ]
        };
        let mut slots = vec![
            full(24),
            full(24),
            full(23),
            // Read at 18% health: a name, but no health bar red enough to find.
            vec![Band {
                top: 10,
                height: 13,
                kind: BandKind::Name,
            }],
            Vec::new(),
        ];

        harmonize(&mut slots);

        let filled = slots[3]
            .iter()
            .find(|b| b.kind == BandKind::Health)
            .expect("slot with a name should inherit the health band");
        assert_eq!((filled.top, filled.height), (24, 18));
        assert!(slots[4].is_empty(), "an empty frame invents nothing");
    }

    /// Two readable slots agree about nothing worth acting on.
    #[test]
    fn harmonize_needs_enough_slots_to_have_an_opinion() {
        let mut slots = vec![
            vec![Band {
                top: 10,
                height: 6,
                kind: BandKind::Name,
            }],
            vec![Band {
                top: 30,
                height: 6,
                kind: BandKind::Name,
            }],
        ];
        harmonize(&mut slots);

        assert_eq!(slots[1][0].top, 30, "too few slots to override anything");
    }
}
