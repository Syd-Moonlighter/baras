//! Dxun Arrow timer trigger.
//!
//! Line crossing is not logged, so first-pack aggro is used instead.

use baras_core::Position;

pub(super) const DXUN_AREA_ID: i64 = 833_571_547_775_792;

#[derive(Debug, Clone, Copy)]
struct PositionBox {
    min_x: f32,
    max_x: f32,
    min_y: f32,
    max_y: f32,
    min_z: f32,
    max_z: f32,
}

impl PositionBox {
    fn contains(self, position: Position) -> bool {
        position.x >= self.min_x
            && position.x <= self.max_x
            && position.y >= self.min_y
            && position.y <= self.max_y
            && position.z >= self.min_z
            && position.z <= self.max_z
    }
}

/// Limits player checks to the entrance corridor.
const ENTRANCE_CORRIDOR: PositionBox = PositionBox {
    min_x: -365.0,
    max_x: -325.0,
    min_y: 120.0,
    max_y: 165.0,
    min_z: -3.0,
    max_z: 3.0,
};

/// First-pack bounds, excluding later and underground shrieks.
const FIRST_ADD_PACK: PositionBox = PositionBox {
    min_x: -350.0,
    max_x: -295.0,
    min_y: 60.0,
    max_y: 120.0,
    min_z: -3.0,
    max_z: 3.0,
};

/// Diagonal start line. Safe is `x - y <= -490`; crossed is greater.
const START_LINE_X_MINUS_Y: f32 = -490.0;

fn player_is_beyond_start_line(position: Position) -> bool {
    ENTRANCE_CORRIDOR.contains(position) && position.x - position.y > START_LINE_X_MINUS_Y
}

/// Matches a first-pack NPC targeting a player beyond the start line.
pub(super) fn matches_first_pack_pull(
    add_position: Position,
    targeted_player_position: Position,
) -> bool {
    FIRST_ADD_PACK.contains(add_position) && player_is_beyond_start_line(targeted_player_position)
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
    fn july_22_first_pull_matches() {
        let shriek = position(-330.37, 104.86, -1.28);
        let runner = position(-337.78, 133.43, 0.40);

        assert!(matches_first_pack_pull(shriek, runner));
    }

    #[test]
    fn safe_staging_players_do_not_match() {
        let shriek = position(-330.37, 104.86, -1.28);
        let safe_positions = [
            position(-347.05, 153.49, -0.09),
            position(-351.65, 154.32, 0.18),
            position(-347.04, 156.62, 0.06),
        ];

        for safe in safe_positions {
            assert!(!matches_first_pack_pull(shriek, safe));
        }
    }

    #[test]
    fn other_observed_crossed_positions_match_the_line_side() {
        let shriek = position(-330.37, 104.86, -1.28);

        assert!(matches_first_pack_pull(
            shriek,
            position(-334.27, 135.01, 0.0),
        ));
        assert!(matches_first_pack_pull(
            shriek,
            position(-331.38, 143.21, 1.01),
        ));
    }

    #[test]
    fn later_surface_pack_does_not_match() {
        let later_shriek = position(-323.35, 27.02, -0.59);
        let player = position(-324.23, 56.13, -0.60);

        assert!(!matches_first_pack_pull(later_shriek, player));
    }

    #[test]
    fn deeper_pack_does_not_match() {
        let deeper_shriek = position(-547.17, 52.92, -22.95);
        let player = position(-527.56, 33.86, -23.53);

        assert!(!matches_first_pack_pull(deeper_shriek, player));
    }

    #[test]
    fn unrelated_npc_outside_first_pack_does_not_match() {
        let unrelated_npc = position(-290.0, 104.0, 0.0);
        let runner = position(-337.78, 133.43, 0.40);

        assert!(!matches_first_pack_pull(unrelated_npc, runner));
    }
}
