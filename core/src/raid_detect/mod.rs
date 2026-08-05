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
pub use matching::{MatchConfig, RowAssignment, RowObservation, assign_rows};
pub use normalize::{edit_distance, normalize, similarity};

/// OCR readings shorter than this are too easy to get from background noise.
pub const MIN_OCR_NAME_CHARS: usize = 3;
