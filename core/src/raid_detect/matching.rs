//! Score names and health readings, then assign at most one player per row.
//!
//! Ambiguous rows stay empty. A missed row is cheaper than a wrong one.

use super::MIN_OCR_NAME_CHARS;
use super::candidates::PlayerCandidate;
use super::normalize::{normalize, similarity};

/// What OCR found in one raid-frame row.
#[derive(Debug, Clone, Default)]
pub struct RowObservation {
    /// Slot index this row corresponds to, as laid out by the raid overlay grid.
    pub row: usize,
    /// Raw recognized name text, in whatever form OCR produced it.
    pub name_text: Option<String>,
    /// Absolute health, e.g. `271245` read from `271,245`.
    pub hp_value: Option<u32>,
    /// Health percentage, e.g. `55` read from `(55%)`.
    pub hp_percent: Option<u8>,
}

/// A row confidently matched to a player.
#[derive(Debug, Clone)]
pub struct RowAssignment {
    pub row: usize,
    pub entity_id: i64,
    /// Exact log spelling, never the OCR output.
    pub name: String,
    pub confidence: f32,
}

/// Thresholds and signal weights.
#[derive(Debug, Clone)]
pub struct MatchConfig {
    /// Minimum name score for a row to be considered at all.
    pub min_confidence: f32,
    /// Minimum lead over the next-best candidate.
    pub min_margin: f32,
    pub name_weight: f32,
    pub hp_value_weight: f32,
    pub hp_percent_weight: f32,
}

impl Default for MatchConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.62,
            min_margin: 0.10,
            // Percentages collide often; names and absolute health do not.
            name_weight: 1.0,
            hp_value_weight: 0.9,
            hp_percent_weight: 0.25,
        }
    }
}

/// Health below this score is too weak to be useful.
const STRONG_HEALTH: f32 = 0.75;

/// Score recognized name text against a candidate.
///
/// SWTOR clips long names at a fixed pixel width, so a shorter reading is
/// compared against the candidate's prefix rather than penalized for being
/// short — `TESTCHARL` matches `TESTCHARLIELONG` on its first nine characters.
fn name_score(observed: &str, candidate: &PlayerCandidate) -> Option<f32> {
    let target = &candidate.normalized;
    if target.is_empty() || observed.len() < MIN_OCR_NAME_CHARS {
        return None;
    }
    if observed == target {
        return Some(1.0);
    }

    // The crop's left edge sometimes yields a stray glyph that normalization
    // keeps because it reads as a letter — `ISOA` for `SOA`. Only an exact match
    // on the remainder counts, so this can never inflate the score of a
    // genuinely different candidate.
    if observed.len() > MIN_OCR_NAME_CHARS && &observed[1..] == target {
        return Some(1.0);
    }

    // `normalize` yields ASCII alphanumerics only, so byte slicing is safe.
    if observed.len() < target.len() {
        Some(similarity(observed, &target[..observed.len()]))
    } else if observed.len() > target.len() && target.len() >= MIN_OCR_NAME_CHARS {
        // Markers and status icons sometimes leave junk after an otherwise good
        // read. Compare the candidate-length prefix as well as the whole line.
        Some(similarity(observed, target).max(similarity(&observed[..target.len()], target)))
    } else {
        Some(similarity(observed, target))
    }
}

/// Score a health reading against a candidate.
///
/// Compared as digit strings rather than numerically: a single misread digit
/// should cost a little, not push the value into a different order of magnitude.
/// Max health is checked as well as current, because out of combat the frame
/// shows exactly the max and because the log reading can lag the screen by a
/// fraction of a second.
fn hp_value_score(observed: u32, candidate: &PlayerCandidate) -> f32 {
    let observed = observed.to_string();

    let mut best: f32 = 0.0;
    if candidate.current_hp > 0 {
        best = best.max(similarity(&observed, &candidate.current_hp.to_string()));
    }
    if candidate.max_hp > 0 {
        // Slight discount: matching current health is the stronger claim.
        best = best.max(similarity(&observed, &candidate.max_hp.to_string()) * 0.95);
    }
    best
}

/// Score a percentage reading, tolerating rounding either side of the boundary.
fn hp_percent_score(observed: u8, candidate: &PlayerCandidate) -> f32 {
    let Some(actual) = candidate.hp_percent() else {
        return 0.0;
    };
    match observed.abs_diff(actual) {
        0 => 1.0,
        1 => 0.8,
        2 => 0.5,
        3 => 0.25,
        _ => 0.0,
    }
}

/// Combined score for one row against one candidate, in `0.0..=1.0`.
fn combined_score(
    observation: &RowObservation,
    normalized_name: Option<&str>,
    candidate: &PlayerCandidate,
    config: &MatchConfig,
) -> f32 {
    let Some(name) = normalized_name else {
        return 0.0;
    };
    let Some(name_score) = name_score(name, candidate) else {
        return 0.0;
    };

    // Health is supporting evidence, never identity. It cannot rescue a name
    // that is too weak to stand on its own.
    if name_score < config.min_confidence {
        return 0.0;
    }

    if config.name_weight <= 0.0 {
        return name_score;
    }
    let mut weighted = name_score * config.name_weight;
    let mut total_weight = config.name_weight;

    // Log health is an event-driven snapshot and may lag behind the frame. Only
    // agreement helps; disagreement and missing health say nothing.
    if let Some(hp) = observation.hp_value
        && (candidate.current_hp > 0 || candidate.max_hp > 0)
    {
        let s = hp_value_score(hp, candidate);
        if s >= STRONG_HEALTH && config.hp_value_weight > 0.0 {
            weighted += s * config.hp_value_weight;
            total_weight += config.hp_value_weight;
        }
    }
    if let Some(pct) = observation.hp_percent
        && candidate.hp_percent().is_some()
    {
        let s = hp_percent_score(pct, candidate);
        if s >= STRONG_HEALTH && config.hp_percent_weight > 0.0 {
            weighted += s * config.hp_percent_weight;
            total_weight += config.hp_percent_weight;
        }
    }

    // Supporting evidence may improve a name score, but must never reduce it.
    name_score.max(weighted / total_weight).clamp(0.0, 1.0)
}

/// Match raid-frame rows to log players.
///
/// Returns confident assignments in row order. Missing rows are left alone.
pub fn assign_rows(
    observations: &[RowObservation],
    candidates: &[PlayerCandidate],
    config: &MatchConfig,
) -> Vec<RowAssignment> {
    if observations.is_empty() || candidates.is_empty() {
        return Vec::new();
    }

    // Normalize each reading once rather than per candidate.
    let normalized: Vec<Option<String>> = observations
        .iter()
        .map(|o| {
            o.name_text
                .as_deref()
                .map(normalize)
                .filter(|n| !n.is_empty())
        })
        .collect();

    // scores[row][candidate]
    let scores: Vec<Vec<f32>> = observations
        .iter()
        .zip(&normalized)
        .map(|(obs, norm)| {
            candidates
                .iter()
                .map(|c| combined_score(obs, norm.as_deref(), c, config))
                .collect()
        })
        .collect();

    let assigned = solve_assignment(&scores, config.min_confidence);

    // Do not let assignment by elimination turn two near-ties into two guesses.
    let mut out = Vec::new();
    for (row_idx, &candidate_idx) in assigned.iter().enumerate() {
        let Some(candidate_idx) = candidate_idx else {
            continue;
        };
        let score = scores[row_idx][candidate_idx];

        let runner_up = scores[row_idx]
            .iter()
            .enumerate()
            .filter(|&(i, _)| i != candidate_idx)
            .map(|(_, &s)| s)
            .fold(0.0f32, f32::max);

        if score - runner_up < config.min_margin {
            continue;
        }

        out.push(RowAssignment {
            row: observations[row_idx].row,
            entity_id: candidates[candidate_idx].entity_id,
            name: candidates[candidate_idx].name.clone(),
            confidence: score,
        });
    }

    out
}

/// Greedy assignment with pairwise score improvements.
fn solve_assignment(scores: &[Vec<f32>], floor: f32) -> Vec<Option<usize>> {
    let rows = scores.len();
    let cols = scores.first().map_or(0, |r| r.len());

    let mut pairs: Vec<(usize, usize, f32)> = Vec::with_capacity(rows * cols);
    for (r, row) in scores.iter().enumerate() {
        for (c, &s) in row.iter().enumerate() {
            if s >= floor {
                pairs.push((r, c, s));
            }
        }
    }
    pairs.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
            .then_with(|| a.1.cmp(&b.1))
    });

    let mut assigned: Vec<Option<usize>> = vec![None; rows];
    let mut taken = vec![false; cols];
    for (r, c, _) in pairs {
        if assigned[r].is_none() && !taken[c] {
            assigned[r] = Some(c);
            taken[c] = true;
        }
    }

    // Swap when the pair's total score improves.
    let mut improved = true;
    while improved {
        improved = false;
        for i in 0..rows {
            for j in (i + 1)..rows {
                let (Some(ci), Some(cj)) = (assigned[i], assigned[j]) else {
                    continue;
                };
                let current = scores[i][ci] + scores[j][cj];
                let swapped = scores[i][cj] + scores[j][ci];
                if swapped > current && scores[i][cj] >= floor && scores[j][ci] >= floor {
                    assigned[i] = Some(cj);
                    assigned[j] = Some(ci);
                    improved = true;
                }
            }
        }
    }

    assigned
}
