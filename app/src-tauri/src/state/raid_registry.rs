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

/// Tracks persistent player-to-slot assignments for raid frames.
///
/// Players are added when they receive an effect from the local player.
/// Players stay in their assigned slot until explicitly removed by user action.
#[derive(Debug, Default)]
pub struct RaidSlotRegistry {
    /// Maps slot (0-15) → registered player info
    slots: HashMap<u8, RegisteredPlayer>,
    /// Names read before a combat-log roster is available.
    provisional_slots: HashMap<u8, String>,
    /// Per slot: last provisional name, and passes survived unclaimed.
    provisional_age: HashMap<u8, (String, u8)>,
    /// Slots whose reading fitted two or more players equally well.
    ambiguous_slots: HashSet<u8>,
    /// Reverse lookup: entity_id → slot
    entity_to_slot: HashMap<i64, u8>,
    /// Maximum number of slots (configurable, default 8)
    max_slots: u8,
    /// Pending discipline info for entities not yet registered
    /// (DisciplineChanged often fires before player is registered)
    /// Maps entity_id -> (class_id, discipline_id)
    pending_disciplines: HashMap<i64, (i64, i64)>,
}

impl RaidSlotRegistry {
    pub fn new(max_slots: u8) -> Self {
        Self {
            slots: HashMap::new(),
            provisional_slots: HashMap::new(),
            provisional_age: HashMap::new(),
            ambiguous_slots: HashSet::new(),
            entity_to_slot: HashMap::new(),
            max_slots,
            pending_disciplines: HashMap::new(),
        }
    }

    /// Try to register a player in the first available slot.
    /// Returns `Some(slot)` if newly registered, `None` if already registered or full.
    /// This is the primary registration method - duplicates are silently rejected.
    /// Any pending discipline info is automatically applied upon registration.
    pub fn try_register(&mut self, entity_id: i64, name: String) -> Option<u8> {
        // Already registered - reject
        if self.entity_to_slot.contains_key(&entity_id) {
            return None;
        }

        let normalized = baras_core::raid_detect::normalize(&name);
        let provisional_slot = self
            .provisional_slots
            .iter()
            .filter_map(|(&slot, provisional)| {
                let read = baras_core::raid_detect::normalize(provisional);
                Some((slot, name_match(&read, &normalized)?))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(slot, _)| slot);
        let slot = provisional_slot.or_else(|| self.find_first_available_slot())?;
        self.provisional_slots.remove(&slot);
        // A real player settles whatever the reading could not.
        self.ambiguous_slots.remove(&slot);
        let mut player = RegisteredPlayer::new(entity_id, name);

        // Check for pending discipline info (DisciplineChanged often fires before registration)
        if let Some((class_id, discipline_id)) = self.pending_disciplines.remove(&entity_id) {
            player.class_id = Some(class_id);
            player.discipline_id = Some(discipline_id);
        }

        self.slots.insert(slot, player);
        self.entity_to_slot.insert(entity_id, slot);
        Some(slot)
    }

    /// Update player's class/discipline from DisciplineChanged event.
    /// If the player isn't registered yet, stores both class and discipline for later application.
    pub fn update_discipline(&mut self, entity_id: i64, class_id: i64, discipline_id: i64) {
        if let Some(&slot) = self.entity_to_slot.get(&entity_id) {
            // Player is registered - update directly
            if let Some(player) = self.slots.get_mut(&slot) {
                player.class_id = Some(class_id);
                player.discipline_id = Some(discipline_id);
            }
        } else {
            // Player not registered yet - store both class_id and discipline_id for later
            self.pending_disciplines
                .insert(entity_id, (class_id, discipline_id));
        }
    }

    /// Update player's name (if we get better info later)
    pub fn update_name(&mut self, entity_id: i64, name: String) {
        if let Some(&slot) = self.entity_to_slot.get(&entity_id)
            && let Some(player) = self.slots.get_mut(&slot)
        {
            player.name = name;
        }
    }

    /// Find the first available slot (lowest numbered empty slot)
    fn find_first_available_slot(&self) -> Option<u8> {
        (0..self.max_slots).find(|&slot| {
            !self.slots.contains_key(&slot) && !self.provisional_slots.contains_key(&slot)
        })
    }

    /// Update OCR-only slots without touching log-backed players.
    pub fn assign_provisional_slots(
        &mut self,
        assignments: impl IntoIterator<Item = (u8, String)>,
    ) {
        self.provisional_slots.clear();
        let mut seen_slots = HashSet::new();
        let mut seen_names = HashSet::new();
        let registered: Vec<_> = self
            .slots
            .values()
            .map(|player| baras_core::raid_detect::normalize(&player.name))
            .collect();

        for (slot, name) in assignments {
            let name = name.trim();
            let normalized = baras_core::raid_detect::normalize(name);
            if slot >= self.max_slots
                || self.slots.contains_key(&slot)
                || normalized.len() < baras_core::raid_detect::MIN_OCR_NAME_CHARS
                || registered
                    .iter()
                    .any(|log| name_match(&normalized, log).is_some())
                || !seen_slots.insert(slot)
                || !seen_names.insert(normalized)
            {
                continue;
            }
            self.provisional_slots.insert(slot, name.to_string());
        }

        // This pass is what proves a name unclaimable.
        for (slot, name) in self.age_provisional_slots() {
            // The name says what OCR keeps misreading.
            tracing::info!(
                slot,
                name = %name,
                passes = PROVISIONAL_MAX_PASSES,
                "Dropped provisional name never claimed by a player"
            );
        }
    }

    /// Apply a detection batch without losing metadata during swaps.
    pub fn assign_slots(
        &mut self,
        assignments: impl IntoIterator<Item = (u8, i64, String)>,
    ) {
        let mut seen_slots = HashSet::new();
        let mut seen_entities = HashSet::new();
        let assignments: Vec<_> = assignments
            .into_iter()
            .filter(|(slot, entity_id, _)| {
                *slot < self.max_slots
                    && seen_slots.insert(*slot)
                    && seen_entities.insert(*entity_id)
            })
            .collect();

        let mut players = HashMap::new();
        for (_, entity_id, _) in &assignments {
            let Some(old_slot) = self.entity_to_slot.remove(entity_id) else {
                continue;
            };
            if let Some(player) = self.slots.remove(&old_slot) {
                players.insert(*entity_id, player);
            }
        }

        for (slot, _, _) in &assignments {
            self.provisional_slots.remove(slot);
            if let Some(displaced) = self.slots.remove(slot) {
                self.entity_to_slot.remove(&displaced.entity_id);
                if let (Some(class_id), Some(discipline_id)) =
                    (displaced.class_id, displaced.discipline_id)
                {
                    self.pending_disciplines
                        .insert(displaced.entity_id, (class_id, discipline_id));
                }
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

            self.slots.insert(slot, player);
            self.entity_to_slot.insert(entity_id, slot);
        }
    }

    /// Swap two slots (user-initiated rearrange)
    pub fn swap_slots(&mut self, slot_a: u8, slot_b: u8) {
        let player_a = self.slots.remove(&slot_a);
        let player_b = self.slots.remove(&slot_b);
        let provisional_a = self.provisional_slots.remove(&slot_a);
        let provisional_b = self.provisional_slots.remove(&slot_b);

        if let Some(p) = player_a {
            self.entity_to_slot.insert(p.entity_id, slot_b);
            self.slots.insert(slot_b, p);
        }
        if let Some(p) = player_b {
            self.entity_to_slot.insert(p.entity_id, slot_a);
            self.slots.insert(slot_a, p);
        }
        if let Some(name) = provisional_a {
            self.provisional_slots.insert(slot_b, name);
        }
        if let Some(name) = provisional_b {
            self.provisional_slots.insert(slot_a, name);
        }
    }

    /// Remove player from a specific slot (user-initiated delete)
    pub fn remove_slot(&mut self, slot: u8) {
        self.provisional_slots.remove(&slot);
        if let Some(player) = self.slots.remove(&slot) {
            self.entity_to_slot.remove(&player.entity_id);
        }
    }

    /// Get the slot for an entity (if registered)
    pub fn get_slot(&self, entity_id: i64) -> Option<u8> {
        self.entity_to_slot.get(&entity_id).copied()
    }

    /// Get the player in a specific slot
    pub fn get_player(&self, slot: u8) -> Option<&RegisteredPlayer> {
        self.slots.get(&slot)
    }

    pub fn get_provisional(&self, slot: u8) -> Option<&str> {
        self.provisional_slots.get(&slot).map(String::as_str)
    }

    pub fn has_provisional(&self) -> bool {
        !self.provisional_slots.is_empty()
    }

    /// Every OCR-only slot, for matching against the log roster.
    pub fn provisional_entries(&self) -> Vec<(u8, String)> {
        let mut entries: Vec<(u8, String)> = self
            .provisional_slots
            .iter()
            .map(|(&slot, name)| (slot, name.clone()))
            .collect();
        entries.sort_by_key(|(slot, _)| *slot);
        entries
    }

    pub fn provisional_len(&self) -> usize {
        self.provisional_slots.len()
    }

    /// Mark the slots a reading could not choose a player for.
    pub fn set_ambiguous_slots(&mut self, slots: impl IntoIterator<Item = u8>) {
        self.ambiguous_slots = slots.into_iter().collect();
    }

    pub fn is_ambiguous(&self, slot: u8) -> bool {
        self.ambiguous_slots.contains(&slot)
    }

    /// Drop provisional names that keep coming back unclaimed.
    ///
    /// A misread matching nobody would otherwise hold its slot for the session
    /// and keep the real player out. A new reading restarts the count.
    fn age_provisional_slots(&mut self) -> Vec<(u8, String)> {
        let age = &mut self.provisional_age;
        let mut dropped = Vec::new();

        self.provisional_slots.retain(|slot, name| {
            let normalized = baras_core::raid_detect::normalize(name);
            let entry = age
                .entry(*slot)
                .or_insert_with(|| (normalized.clone(), 0));
            if entry.0 == normalized {
                entry.1 += 1;
            } else {
                *entry = (normalized, 0);
            }
            let keep = entry.1 <= PROVISIONAL_MAX_PASSES;
            if !keep {
                dropped.push((*slot, name.clone()));
            }
            keep
        });

        age.retain(|slot, _| self.provisional_slots.contains_key(slot));
        dropped
    }

    pub fn registered_len(&self) -> usize {
        self.slots.len()
    }

    /// Check if a player is registered
    pub fn is_registered(&self, entity_id: i64) -> bool {
        self.entity_to_slot.contains_key(&entity_id)
    }

    /// Clear all assignments (new session/encounter)
    pub fn clear(&mut self) {
        self.slots.clear();
        self.provisional_slots.clear();
        self.entity_to_slot.clear();
        self.pending_disciplines.clear();
    }

    /// Iterate over all registered players with their slots
    pub fn iter(&self) -> impl Iterator<Item = (u8, &RegisteredPlayer)> {
        self.slots.iter().map(|(&slot, player)| (slot, player))
    }

    /// Number of registered players
    pub fn len(&self) -> usize {
        self.slots.len() + self.provisional_slots.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty() && self.provisional_slots.is_empty()
    }

    /// Maximum slots configured
    pub fn max_slots(&self) -> u8 {
        self.max_slots
    }

    /// Update max slots and compact players if grid shrinks.
    /// Players in slots >= new_max are moved to available lower slots.
    /// Returns the number of players that couldn't fit and were removed.
    pub fn set_max_slots(&mut self, new_max: u8) -> usize {
        if new_max >= self.max_slots {
            self.max_slots = new_max;
            return 0;
        }

        let mut displaced: Vec<RegisteredPlayer> = Vec::new();
        let mut provisional: Vec<_> = self.provisional_slots.drain().collect();
        provisional.sort_by_key(|(slot, _)| *slot);
        let mut slots_to_remove = Vec::new();

        for &slot in self.slots.keys() {
            if slot >= new_max {
                slots_to_remove.push(slot);
            }
        }

        for slot in slots_to_remove {
            if let Some(player) = self.slots.remove(&slot) {
                self.entity_to_slot.remove(&player.entity_id);
                displaced.push(player);
            }
        }
        self.max_slots = new_max;

        let mut removed_count = 0;
        for player in displaced {
            if let Some(new_slot) = self.find_first_available_slot() {
                let entity_id = player.entity_id;
                self.slots.insert(new_slot, player);
                self.entity_to_slot.insert(entity_id, new_slot);
            } else {
                removed_count += 1;
            }
        }
        for (old_slot, name) in provisional {
            let slot = (old_slot < new_max
                && !self.slots.contains_key(&old_slot)
                && !self.provisional_slots.contains_key(&old_slot))
                .then_some(old_slot)
                .or_else(|| self.find_first_available_slot());
            if let Some(slot) = slot {
                self.provisional_slots.insert(slot, name);
            } else {
                removed_count += 1;
            }
        }

        removed_count
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
}
