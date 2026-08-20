//! Gods from the Machine Doom's Delay trigger.

use baras_core::{Position, game_data::Difficulty};

pub(super) const GODS_AREA_ID: i64 = 833_571_547_775_765;
pub(super) const TIMER_NAME: &str = "Doom's Delay";
pub(super) const FIRST_ADD_PULL_OFFSET_SECS: u64 = 5;

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

const TELEPORTER_DESTINATION: PositionBox = PositionBox {
    min_x: 805.0,
    max_x: 825.0,
    min_y: 1825.0,
    max_y: 1860.0,
    min_z: 230.0,
    max_z: 240.0,
};

const FIRST_ADD_PACK: PositionBox = PositionBox {
    min_x: 800.0,
    max_x: 835.0,
    min_y: 1880.0,
    max_y: 1915.0,
    min_z: 228.0,
    max_z: 238.0,
};

const ENTRANCE_SECTION: PositionBox = PositionBox {
    min_x: 795.0,
    max_x: 840.0,
    min_y: 1825.0,
    max_y: 1915.0,
    min_z: 228.0,
    max_z: 240.0,
};

pub(super) fn matches_area(area_id: Option<i64>, difficulty: Option<Difficulty>) -> bool {
    area_id == Some(GODS_AREA_ID)
        && matches!(difficulty, Some(Difficulty::Master8 | Difficulty::Master16))
}

pub(super) fn matches_teleporter_destination(
    area_id: Option<i64>,
    difficulty: Option<Difficulty>,
    position: Position,
) -> bool {
    matches_area(area_id, difficulty) && TELEPORTER_DESTINATION.contains(position)
}

pub(super) fn matches_first_add_pull(
    area_id: Option<i64>,
    difficulty: Option<Difficulty>,
    add_position: Position,
    targeted_player_position: Position,
) -> bool {
    matches_area(area_id, difficulty)
        && FIRST_ADD_PACK.contains(add_position)
        && ENTRANCE_SECTION.contains(targeted_player_position)
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
    fn observed_teleporter_destination_matches() {
        for destination in [
            position(815.07, 1837.49, 234.74),
            position(815.29, 1850.91, 232.44),
            position(815.07, 1832.30, 236.21),
        ] {
            assert!(matches_teleporter_destination(
                Some(GODS_AREA_ID),
                Some(Difficulty::Master8),
                destination,
            ));
        }
    }

    #[test]
    fn staging_and_later_positions_do_not_match_teleporter() {
        for position in [
            position(63.84, 1553.43, 229.61),
            position(818.56, 1891.32, 232.43),
            position(837.96, 2048.13, 239.22),
        ] {
            assert!(!matches_teleporter_destination(
                Some(GODS_AREA_ID),
                Some(Difficulty::Master8),
                position,
            ));
        }
    }

    #[test]
    fn observed_first_add_pull_matches() {
        let player = position(818.56, 1891.32, 232.43);
        for add in [
            position(816.01, 1900.41, 232.43),
            position(815.67, 1908.20, 232.43),
            position(826.68, 1901.40, 231.98),
            position(826.33, 1897.69, 231.98),
        ] {
            assert!(matches_first_add_pull(
                Some(GODS_AREA_ID),
                Some(Difficulty::Master8),
                add,
                player,
            ));
        }
    }

    #[test]
    fn later_and_unrelated_adds_do_not_match() {
        let player = position(818.56, 1891.32, 232.43);

        assert!(!matches_first_add_pull(
            Some(GODS_AREA_ID),
            Some(Difficulty::Master8),
            position(810.37, 2102.05, 244.93),
            player,
        ));
        assert!(!matches_first_add_pull(
            Some(GODS_AREA_ID),
            Some(Difficulty::Master8),
            position(775.0, 1900.0, 232.43),
            player,
        ));
    }

    #[test]
    fn triggers_require_gods_master_mode() {
        let destination = position(815.07, 1837.49, 234.74);

        assert!(!matches_teleporter_destination(
            Some(GODS_AREA_ID),
            Some(Difficulty::Story8),
            destination,
        ));
        assert!(!matches_teleporter_destination(
            Some(0),
            Some(Difficulty::Master8),
            destination,
        ));
    }
}
