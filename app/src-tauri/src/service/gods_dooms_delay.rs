//! Gods from the Machine Doom's Delay trigger.

use baras_core::game_data::Difficulty;

pub(super) const GODS_AREA_ID: i64 = 833_571_547_775_765;
pub(super) const TIMER_NAME: &str = "Doom's Delay";

const FORGE_APPROACH: &str = "Forge Approach";

pub(super) fn is_forge_approach(area_name: &str) -> bool {
    area_name.trim().eq_ignore_ascii_case(FORGE_APPROACH)
}

pub(super) fn matches_area_entry(area_name: &str, difficulty_id: i64) -> bool {
    is_forge_approach(area_name)
        && matches!(
            Difficulty::from_difficulty_id(difficulty_id),
            Some(Difficulty::Master8 | Difficulty::Master16)
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const VETERAN_8: i64 = 836_045_448_953_652;
    const MASTER_8: i64 = 836_045_448_953_655;
    const MASTER_16: i64 = 836_045_448_953_656;

    #[test]
    fn forge_approach_master_matches() {
        assert!(matches_area_entry("Forge Approach", MASTER_8));
        assert!(matches_area_entry(" forge approach ", MASTER_16));
    }

    #[test]
    fn other_maps_do_not_match() {
        assert!(!matches_area_entry("Valley of the Machine Gods", MASTER_8));
    }

    #[test]
    fn non_master_does_not_match() {
        assert!(!matches_area_entry("Forge Approach", VETERAN_8));
    }
}
