//! R-4 Anomalously Skilled timer trigger.

use baras_core::{Position, game_data::Difficulty};

pub(super) const R4_AREA_ID: i64 = 833_571_547_775_799;
pub(super) const TIMER_NAME: &str = "Anomalously Skilled";

const START_LINE_X: f32 = 288.0;
const MIN_Y: f32 = 8.0;
const MAX_Y: f32 = 23.0;
const MIN_Z: f32 = 424.0;
const MAX_Z: f32 = 431.0;

pub(super) fn matches_player_position(
    area_id: Option<i64>,
    difficulty: Option<Difficulty>,
    position: Position,
) -> bool {
    area_id == Some(R4_AREA_ID)
        && matches!(
            difficulty,
            Some(Difficulty::Veteran8 | Difficulty::Veteran16)
        )
        && position.x < START_LINE_X
        && position.y >= MIN_Y
        && position.y <= MAX_Y
        && position.z >= MIN_Z
        && position.z <= MAX_Z
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position(x: f32, y: f32, z: f32) -> Position {
        Position {
            x,
            y,
            z,
            facing: 0.0,
        }
    }

    #[test]
    fn observed_crossed_player_matches() {
        assert!(matches_player_position(
            Some(R4_AREA_ID),
            Some(Difficulty::Veteran8),
            position(286.10, 14.86, 427.36),
        ));
    }

    #[test]
    fn players_before_the_line_do_not_match() {
        for safe in [
            position(355.53, 17.67, 427.10),
            position(292.89, 15.91, 427.10),
            position(288.22, 15.46, 427.35),
        ] {
            assert!(!matches_player_position(
                Some(R4_AREA_ID),
                Some(Difficulty::Veteran8),
                safe,
            ));
        }
    }

    #[test]
    fn positions_outside_the_line_segment_do_not_match() {
        let outside_y = position(286.0, 24.0, 427.35);
        let lower_level = position(257.13, 10.68, 397.45);

        for position in [outside_y, lower_level] {
            assert!(!matches_player_position(
                Some(R4_AREA_ID),
                Some(Difficulty::Veteran8),
                position,
            ));
        }
    }

    #[test]
    fn non_veteran_and_other_areas_do_not_match() {
        let crossed = position(286.10, 14.86, 427.36);

        assert!(!matches_player_position(
            Some(R4_AREA_ID),
            Some(Difficulty::Master8),
            crossed,
        ));
        assert!(!matches_player_position(
            Some(0),
            Some(Difficulty::Veteran8),
            crossed,
        ));
    }
}
