//! Match OCR observations to players from the combat log.
//!
//! Capture and recognition live elsewhere so matching stays cheap to test.

mod candidates;
mod matching;
mod normalize;

#[cfg(test)]
mod matching_tests;
#[cfg(test)]
mod normalize_tests;

pub use candidates::{CandidateSet, PlayerCandidate};
pub use matching::{
    CandidateScore, Contribution, MatchConfig, RowAssignment, RowDecision, RowObservation,
    assign_rows, assign_rows_explained, name_similarity,
};
pub use normalize::{edit_distance, normalize, similarity};

/// OCR readings shorter than this are too easy to get from background noise.
pub const MIN_OCR_NAME_CHARS: usize = 3;

/// Below this a reading is not the same name.
pub const MIN_NAME_CONFIDENCE: f32 = 0.62;
