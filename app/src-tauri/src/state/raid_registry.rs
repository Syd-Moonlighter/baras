//! Raid slot registry - persistent player-to-slot assignments for raid frames
//!
//! Players are added when they receive an effect from the local player.
//! Players stay in their assigned slot until explicitly removed by user action.

use std::collections::{HashMap, HashSet};

/// Detection passes a provisional may survive unclaimed. Passes, not fights:
/// only a reading that saw it again proves it wrong.
const PROVISIONAL_MAX_PASSES: u8 = 3;

/// Information about a player registered in the raid frame
#[derive(Debug, Clone)]
pub struct RegisteredPlayer {
    pub entity_id: i64,
    pub name: String,
    pub class_id: Option<i64>,
    pub discipline_id: Option<i64>,
}

impl RegisteredPlayer {
    pub fn new(entity_id: i64, name: String) -> Self {
        Self {
            entity_id,
            name,
            class_id: None,
            discipline_id: None,
        }
    }
}

/// How well an OCR reading matches a log name, ignoring anything too weak.
fn name_match(read: &str, log_name: &str) -> Option<f32> {
    baras_core::raid_detect::name_similarity(read, log_name)
        .filter(|score| *score >= baras_core::raid_detect::MIN_NAME_CONFIDENCE)
}

/// What one raid-frame slot holds.
#[derive(Debug, Clone)]
enum SlotOccupant {
    /// A player with a log identity.
    Player(RegisteredPlayer),
    /// A name read before a combat-log roster could claim it.
    Provisional {
        name: String,
        /// Detection passes this name has survived unclaimed.
        passes_unclaimed: u8,
    },
}

/// One slot: its occupant and its per-slot flags, moved as a unit so a swap or
/// clear can never leave an attribute behind.
#[derive(Debug, Clone, Default)]
struct SlotEntry {
    occupant: Option<SlotOccupant>,
    /// Last reading fitted two or more players equally well (display-only).
    ambiguous: bool,
}

/// Tracks persistent player-to-slot assignments for raid frames.
///
/// Players are added when they receive an effect from the local player.
/// Players stay in their assigned slot until explicitly removed by user action.
#[derive(Debug, Default)]
pub struct RaidSlotRegistry {
    /// One entry per slot; the length is the configured maximum.
    slots: Vec<SlotEntry>,
    /// Pending discipline info for entities not yet registered
    /// (DisciplineChanged often fires before player is registered)
    /// Maps entity_id -> (class_id, discipline_id)
    pending_disciplines: HashMap<i64, (i64, i64)>,
}

impl RaidSlotRegistry {
    pub fn new(max_slots: u8) -> Self {
        Self {
            slots: vec![SlotEntry::default(); max_slots as usize],
            pending_disciplines: HashMap::new(),
        }
    }

    /// Try to register a player in the first available slot.
    /// Returns `Some(slot)` if newly registered, `None` if already registered or full.
    /// This is the primary registration method - duplicates are silently rejected.
    /// Any pending discipline info is automatically applied upon registration.
    pub fn try_register(&mut self, entity_id: i64, name: String) -> Option<u8> {
        // Already registered - reject
        if self.slot_of(entity_id).is_some() {
            return None;
        }

        let normalized = baras_core::raid_detect::normalize(&name);
        let provisional_slot = self
            .provisionals()
            .filter_map(|(slot, provisional, _)| {
                let read = baras_core::raid_detect::normalize(provisional);
                Some((slot, name_match(&read, &normalized)?))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(slot, _)| slot);
        let slot = provisional_slot.or_else(|| self.first_free_slot())?;

        let mut player = RegisteredPlayer::new(entity_id, name);
        // Check for pending discipline info (DisciplineChanged often fires before registration)
        if let Some((class_id, discipline_id)) = self.pending_disciplines.remove(&entity_id) {
            player.class_id = Some(class_id);
            player.discipline_id = Some(discipline_id);
        }

        let entry = &mut self.slots[slot as usize];
        entry.occupant = Some(SlotOccupant::Player(player));
        // A real player settles whatever the reading could not.
        entry.ambiguous = false;
        Some(slot)
    }

    /// Update player's class/discipline from DisciplineChanged event.
    /// If the player isn't registered yet, stores both class and discipline for later application.
    pub fn update_discipline(&mut self, entity_id: i64, class_id: i64, discipline_id: i64) {
        match self.player_mut(entity_id) {
            Some(player) => {
                player.class_id = Some(class_id);
                player.discipline_id = Some(discipline_id);
            }
            None => {
                self.pending_disciplines
                    .insert(entity_id, (class_id, discipline_id));
            }
        }
    }

    /// Update OCR-only slots without touching log-backed players.
    ///
    /// Also the aging step: a provisional that keeps coming back unclaimed is a
    /// misread matching nobody, and would otherwise hold its slot for the
    /// session and keep the real player out. A new reading restarts the count.
    pub fn assign_provisional_slots(
        &mut self,
        assignments: impl IntoIterator<Item = (u8, String)>,
    ) {
        // Prior provisionals, so an unchanged name carries its pass count over.
        let prior: Vec<Option<(String, u8)>> = self
            .slots
            .iter_mut()
            .map(|entry| match entry.occupant.take() {
                Some(SlotOccupant::Provisional {
                    name,
                    passes_unclaimed,
                }) => Some((baras_core::raid_detect::normalize(&name), passes_unclaimed)),
                keep => {
                    entry.occupant = keep;
                    None
                }
            })
            .collect();

        let mut seen_slots = HashSet::new();
        let mut seen_names = HashSet::new();
        let registered: Vec<_> = self
            .players()
            .map(|(_, player)| baras_core::raid_detect::normalize(&player.name))
            .collect();

        for (slot, name) in assignments {
            let name = name.trim();
            let normalized = baras_core::raid_detect::normalize(name);
            if slot as usize >= self.slots.len()
                || matches!(
                    self.slots[slot as usize].occupant,
                    Some(SlotOccupant::Player(_))
                )
                || normalized.len() < baras_core::raid_detect::MIN_OCR_NAME_CHARS
                || registered
                    .iter()
                    .any(|log| name_match(&normalized, log).is_some())
                || !seen_slots.insert(slot)
            {
                continue;
            }
            let passes_unclaimed = match &prior[slot as usize] {
                Some((last, passes)) if *last == normalized => passes + 1,
                Some(_) => 0,
                None => 1,
            };
            if !seen_names.insert(normalized) {
                continue;
            }
            if passes_unclaimed > PROVISIONAL_MAX_PASSES {
                // This pass is what proves a name unclaimable; the name says
                // what OCR keeps misreading.
                tracing::info!(
                    slot,
                    name = %name,
                    passes = PROVISIONAL_MAX_PASSES,
                    "Dropped provisional name never claimed by a player"
                );
                continue;
            }
            self.slots[slot as usize].occupant = Some(SlotOccupant::Provisional {
                name: name.to_string(),
                passes_unclaimed,
            });
        }
    }

    /// Apply a detection batch without losing metadata during swaps.
    pub fn assign_slots(&mut self, assignments: impl IntoIterator<Item = (u8, i64, String)>) {
        let mut seen_slots = HashSet::new();
        let mut seen_entities = HashSet::new();
        let assignments: Vec<_> = assignments
            .into_iter()
            .filter(|(slot, entity_id, _)| {
                (*slot as usize) < self.slots.len()
                    && seen_slots.insert(*slot)
                    && seen_entities.insert(*entity_id)
            })
            .collect();

        // Pull assigned players out of their old slots, keeping their metadata.
        let mut players = HashMap::new();
        for (_, entity_id, _) in &assignments {
            if let Some(slot) = self.slot_of(*entity_id)
                && let Some(SlotOccupant::Player(player)) =
                    self.slots[slot as usize].occupant.take()
            {
                players.insert(*entity_id, player);
            }
        }

        // Vacate the target slots; a displaced player keeps its discipline.
        for (slot, _, _) in &assignments {
            if let Some(SlotOccupant::Player(displaced)) =
                self.slots[*slot as usize].occupant.take()
                && let (Some(class_id), Some(discipline_id)) =
                    (displaced.class_id, displaced.discipline_id)
            {
                self.pending_disciplines
                    .insert(displaced.entity_id, (class_id, discipline_id));
            }
        }

        for (slot, entity_id, name) in assignments {
            let mut player = players
                .remove(&entity_id)
                .unwrap_or_else(|| RegisteredPlayer::new(entity_id, name.clone()));
            player.name = name;

            if player.class_id.is_none()
                && let Some((class_id, discipline_id)) =
                    self.pending_disciplines.remove(&entity_id)
            {
                player.class_id = Some(class_id);
                player.discipline_id = Some(discipline_id);
            }

            self.slots[slot as usize].occupant = Some(SlotOccupant::Player(player));
        }
    }

    /// Swap two slots (user-initiated rearrange)
    pub fn swap_slots(&mut self, slot_a: u8, slot_b: u8) {
        let (a, b) = (slot_a as usize, slot_b as usize);
        if a < self.slots.len() && b < self.slots.len() {
            self.slots.swap(a, b);
        }
    }

    /// Remove player from a specific slot (user-initiated delete)
    pub fn remove_slot(&mut self, slot: u8) {
        if let Some(entry) = self.slots.get_mut(slot as usize) {
            *entry = SlotEntry::default();
        }
    }

    /// Get the slot for an entity (if registered)
    pub fn get_slot(&self, entity_id: i64) -> Option<u8> {
        self.slot_of(entity_id)
    }

    /// Get the player in a specific slot
    pub fn get_player(&self, slot: u8) -> Option<&RegisteredPlayer> {
        match self.slots.get(slot as usize)?.occupant.as_ref()? {
            SlotOccupant::Player(player) => Some(player),
            SlotOccupant::Provisional { .. } => None,
        }
    }

    pub fn get_provisional(&self, slot: u8) -> Option<&str> {
        match self.slots.get(slot as usize)?.occupant.as_ref()? {
            SlotOccupant::Provisional { name, .. } => Some(name),
            SlotOccupant::Player(_) => None,
        }
    }

    pub fn has_provisional(&self) -> bool {
        self.provisionals().next().is_some()
    }

    /// Every OCR-only slot in slot order, for matching against the log roster.
    pub fn provisional_entries(&self) -> Vec<(u8, String)> {
        self.provisionals()
            .map(|(slot, name, _)| (slot, name.to_string()))
            .collect()
    }

    pub fn provisional_len(&self) -> usize {
        self.provisionals().count()
    }

    /// Mark the slots a reading could not choose a player for.
    pub fn set_ambiguous_slots(&mut self, slots: impl IntoIterator<Item = u8>) {
        for entry in &mut self.slots {
            entry.ambiguous = false;
        }
        for slot in slots {
            if let Some(entry) = self.slots.get_mut(slot as usize) {
                entry.ambiguous = true;
            }
        }
    }

    pub fn is_ambiguous(&self, slot: u8) -> bool {
        self.slots
            .get(slot as usize)
            .is_some_and(|entry| entry.ambiguous)
    }

    pub fn registered_len(&self) -> usize {
        self.players().count()
    }

    /// Check if a player is registered
    pub fn is_registered(&self, entity_id: i64) -> bool {
        self.slot_of(entity_id).is_some()
    }

    /// Clear all assignments (new session/encounter)
    pub fn clear(&mut self) {
        self.slots.fill(SlotEntry::default());
        self.pending_disciplines.clear();
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.slots.iter().all(|entry| entry.occupant.is_none())
    }

    /// Maximum slots configured
    pub fn max_slots(&self) -> u8 {
        self.slots.len() as u8
    }

    /// Update max slots and compact players if grid shrinks.
    /// Players in slots >= new_max are moved to available lower slots.
    /// Returns the number of players that couldn't fit and were removed.
    pub fn set_max_slots(&mut self, new_max: u8) -> usize {
        if new_max as usize >= self.slots.len() {
            self.slots.resize(new_max as usize, SlotEntry::default());
            return 0;
        }

        // Provisionals re-place after players, so players take priority.
        let provisional: Vec<(u8, SlotOccupant)> = self
            .slots
            .iter_mut()
            .enumerate()
            .filter_map(|(slot, entry)| match entry.occupant {
                Some(SlotOccupant::Provisional { .. }) => {
                    Some((slot as u8, entry.occupant.take().unwrap()))
                }
                _ => None,
            })
            .collect();
        let displaced: Vec<SlotOccupant> = self
            .slots
            .drain(new_max as usize..)
            .filter_map(|entry| entry.occupant)
            .collect();

        let mut removed_count = 0;
        for occupant in displaced {
            match self.first_free_slot() {
                Some(slot) => self.slots[slot as usize].occupant = Some(occupant),
                None => removed_count += 1,
            }
        }
        for (old_slot, occupant) in provisional {
            let slot = ((old_slot as usize) < self.slots.len()
                && self.slots[old_slot as usize].occupant.is_none())
            .then_some(old_slot)
            .or_else(|| self.first_free_slot());
            match slot {
                Some(slot) => self.slots[slot as usize].occupant = Some(occupant),
                None => removed_count += 1,
            }
        }

        removed_count
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Internal views
    // ─────────────────────────────────────────────────────────────────────────

    /// Lowest empty slot.
    fn first_free_slot(&self) -> Option<u8> {
        self.slots
            .iter()
            .position(|entry| entry.occupant.is_none())
            .map(|slot| slot as u8)
    }

    /// Slot holding this entity, if registered. Linear scan: at most 16 slots.
    fn slot_of(&self, entity_id: i64) -> Option<u8> {
        self.players()
            .find(|(_, player)| player.entity_id == entity_id)
            .map(|(slot, _)| slot)
    }

    fn player_mut(&mut self, entity_id: i64) -> Option<&mut RegisteredPlayer> {
        self.slots.iter_mut().find_map(|entry| match &mut entry.occupant {
            Some(SlotOccupant::Player(player)) if player.entity_id == entity_id => Some(player),
            _ => None,
        })
    }

    fn players(&self) -> impl Iterator<Item = (u8, &RegisteredPlayer)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| match &entry.occupant {
                Some(SlotOccupant::Player(player)) => Some((slot as u8, player)),
                _ => None,
            })
    }

    fn provisionals(&self) -> impl Iterator<Item = (u8, &str, u8)> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| match &entry.occupant {
                Some(SlotOccupant::Provisional {
                    name,
                    passes_unclaimed,
                }) => Some((slot as u8, name.as_str(), *passes_unclaimed)),
                _ => None,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_swap_keeps_player_metadata() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.try_register(1, "One".into());
        registry.try_register(2, "Two".into());
        registry.update_discipline(1, 10, 11);
        registry.update_discipline(2, 20, 21);

        registry.assign_slots([(1, 1, "One".into()), (0, 2, "Two".into())]);

        let one = registry.get_player(1).unwrap();
        let two = registry.get_player(0).unwrap();
        assert_eq!((one.class_id, one.discipline_id), (Some(10), Some(11)));
        assert_eq!((two.class_id, two.discipline_id), (Some(20), Some(21)));
        assert_eq!(registry.get_slot(1), Some(1));
        assert_eq!(registry.get_slot(2), Some(0));
    }

    #[test]
    fn detection_remembers_metadata_for_an_evicted_player() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.try_register(1, "One".into());
        registry.try_register(2, "Two".into());
        registry.update_discipline(2, 20, 21);

        registry.assign_slots([(1, 1, "One".into())]);
        registry.assign_slots([(2, 2, "Two".into())]);

        let two = registry.get_player(2).unwrap();
        assert_eq!((two.class_id, two.discipline_id), (Some(20), Some(21)));
    }

    #[test]
    fn provisional_name_is_not_a_registered_entity() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(2, "PLAYER 8K".into())]);

        assert_eq!(registry.get_provisional(2), Some("PLAYER 8K"));
        assert!(!registry.is_registered(2));
        assert!(registry.has_provisional());
    }

    #[test]
    fn provisional_names_use_the_shared_ocr_minimum() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(0, "AI".into()), (1, "BOT".into())]);

        assert_eq!(registry.get_provisional(0), None);
        assert_eq!(registry.get_provisional(1), Some("BOT"));
    }

    #[test]
    fn real_player_can_claim_matching_provisional_slot() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(2, "Alpha".into())]);

        assert_eq!(registry.try_register(42, "ALPHA".into()), Some(2));
        assert_eq!(registry.get_provisional(2), None);
        assert_eq!(registry.get_player(2).map(|p| p.entity_id), Some(42));
    }

    #[test]
    fn a_misread_provisional_name_still_claims_its_slot() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(2, "CATZOON Y".into())]);

        assert_eq!(registry.try_register(42, "Catzoon".into()), Some(2));
        assert_eq!(registry.provisional_len(), 0);
    }

    #[test]
    fn provisional_pass_does_not_replace_real_players() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.try_register(42, "Alpha".into());

        registry.assign_provisional_slots([
            (0, "Wrong".into()),
            (1, "Alpha".into()),
            (2, "Bravo".into()),
        ]);

        assert_eq!(registry.get_player(0).map(|p| p.entity_id), Some(42));
        assert_eq!(registry.get_provisional(0), None);
        assert_eq!(registry.get_provisional(1), None);
        assert_eq!(registry.get_provisional(2), Some("Bravo"));
    }

    #[test]
    fn partial_log_match_keeps_the_unresolved_names_for_the_next_combat() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([
            (0, "Alpha".into()),
            (1, "Bravo".into()),
            (2, "Charlie".into()),
        ]);

        registry.assign_slots([(0, 42, "Alpha".into())]);

        assert_eq!(registry.provisional_len(), 2);
        assert_eq!(registry.get_provisional(1), Some("Bravo"));
        assert_eq!(registry.get_provisional(2), Some("Charlie"));
    }

    #[test]
    fn real_players_take_priority_when_the_grid_shrinks() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(0, "Bravo".into()), (1, "Charlie".into())]);
        registry.assign_slots([(3, 42, "Alpha".into())]);

        assert_eq!(registry.set_max_slots(2), 1);
        assert_eq!(registry.get_player(0).map(|p| p.entity_id), Some(42));
        assert_eq!(registry.provisional_len(), 1);
    }

    #[test]
    fn an_unclaimed_name_is_dropped_after_max_passes() {
        let mut registry = RaidSlotRegistry::new(4);
        for _ in 0..PROVISIONAL_MAX_PASSES {
            registry.assign_provisional_slots([(0, "Ghost".into())]);
        }
        assert_eq!(registry.get_provisional(0), Some("Ghost"));

        registry.assign_provisional_slots([(0, "Ghost".into())]);
        assert_eq!(registry.get_provisional(0), None);
    }

    #[test]
    fn a_new_reading_restarts_the_unclaimed_count() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(0, "Ghost".into())]);
        registry.assign_provisional_slots([(0, "Ghost".into())]);
        registry.assign_provisional_slots([(0, "Rider".into())]);

        // The rename reset the count, so Rider gets its own full run.
        for _ in 0..PROVISIONAL_MAX_PASSES {
            registry.assign_provisional_slots([(0, "Rider".into())]);
            assert_eq!(registry.get_provisional(0), Some("Rider"));
        }
        registry.assign_provisional_slots([(0, "Rider".into())]);
        assert_eq!(registry.get_provisional(0), None);
    }

    #[test]
    fn clear_resets_ambiguity_and_aging() {
        let mut registry = RaidSlotRegistry::new(4);
        for _ in 0..PROVISIONAL_MAX_PASSES {
            registry.assign_provisional_slots([(0, "Ghost".into())]);
        }
        registry.set_ambiguous_slots([0]);

        registry.clear();
        assert!(!registry.is_ambiguous(0));

        // A fresh session's reading gets its full run, not the old count.
        for _ in 0..PROVISIONAL_MAX_PASSES {
            registry.assign_provisional_slots([(0, "Ghost".into())]);
            assert_eq!(registry.get_provisional(0), Some("Ghost"));
        }
    }

    #[test]
    fn swap_moves_ambiguity_with_the_slot() {
        let mut registry = RaidSlotRegistry::new(4);
        registry.assign_provisional_slots([(0, "Alpha".into())]);
        registry.set_ambiguous_slots([0]);

        registry.swap_slots(0, 1);

        assert_eq!(registry.get_provisional(1), Some("Alpha"));
        assert!(registry.is_ambiguous(1));
        assert!(!registry.is_ambiguous(0));
    }
}
