use std::collections::HashMap;

use mongodb::bson::DateTime;
use serde::{Deserialize, Serialize};

use crate::domain::BlocKey;

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct StructureStats {
    destroyed_in_combat: u64,
    destroyed_via_coordination_service: u64,
    destroyed_by_authorized_users: u64,
    built: u64,
}

impl StructureStats {
    pub(crate) fn destroyed_in_combat(&self) -> u64 {
        self.destroyed_in_combat
    }

    pub(crate) fn destroyed_via_coordination_service(&self) -> u64 {
        self.destroyed_via_coordination_service
    }

    pub(crate) fn destroyed_by_authorized_users(&self) -> u64 {
        self.destroyed_by_authorized_users
    }

    pub(crate) fn built(&self) -> u64 {
        self.built
    }

    fn record_destruction(&mut self, source: DestructionSource) {
        match source {
            DestructionSource::Combat => self.destroyed_in_combat += 1,
            DestructionSource::CoordinationService => self.destroyed_via_coordination_service += 1,
            DestructionSource::AuthorizedUser => self.destroyed_by_authorized_users += 1,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct UnitStats {
    destroyed_by_enemies: u64,
    produced: u64,
}

impl UnitStats {
    pub(crate) fn destroyed_by_enemies(&self) -> u64 {
        self.destroyed_by_enemies
    }

    pub(crate) fn produced(&self) -> u64 {
        self.produced
    }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct BlocStats {
    trusts: StructureStats,
    bases: StructureStats,
    units: UnitStats,
}

impl BlocStats {
    pub(crate) fn trusts(&self) -> &StructureStats {
        &self.trusts
    }

    pub(crate) fn bases(&self) -> &StructureStats {
        &self.bases
    }

    pub(crate) fn units(&self) -> &UnitStats {
        &self.units
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SimulationStats {
    started_at: DateTime,
    blocs: HashMap<BlocKey, BlocStats>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DestructionSource {
    Combat,
    CoordinationService,
    AuthorizedUser,
}

impl Default for SimulationStats {
    fn default() -> Self {
        Self {
            started_at: DateTime::now(),
            blocs: HashMap::new(),
        }
    }
}

impl SimulationStats {
    pub(crate) fn runtime_seconds(&self) -> u64 {
        u64::try_from(
            DateTime::now()
                .timestamp_millis()
                .saturating_sub(self.started_at.timestamp_millis()),
        )
        .unwrap_or_default()
            / 1_000
    }

    pub(crate) fn bloc(&self, bloc: &BlocKey) -> BlocStats {
        self.blocs.get(bloc).cloned().unwrap_or_default()
    }

    pub(crate) fn record_base_built(&mut self, bloc: BlocKey) {
        self.blocs.entry(bloc).or_default().bases.built += 1;
    }

    pub(crate) fn record_trust_built(&mut self, bloc: BlocKey) {
        self.blocs.entry(bloc).or_default().trusts.built += 1;
    }

    pub(crate) fn record_units_produced(&mut self, bloc: BlocKey, count: u64) {
        self.blocs.entry(bloc).or_default().units.produced += count;
    }

    pub(crate) fn record_unit_destroyed_by_enemy(&mut self, bloc: BlocKey) {
        self.blocs.entry(bloc).or_default().units.destroyed_by_enemies += 1;
    }

    pub(crate) fn record_base_destroyed(&mut self, bloc: BlocKey, source: DestructionSource) {
        self.blocs.entry(bloc).or_default().bases.record_destruction(source);
    }

    pub(crate) fn record_trust_destroyed(&mut self, bloc: BlocKey, source: DestructionSource) {
        self.blocs.entry(bloc).or_default().trusts.record_destruction(source);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_each_counter_for_the_owning_bloc() {
        let bloc = BlocKey::from("bloc-a".to_string());
        let mut stats = SimulationStats::default();

        stats.record_base_built(bloc.clone());
        stats.record_trust_built(bloc.clone());
        stats.record_units_produced(bloc.clone(), 3);
        stats.record_unit_destroyed_by_enemy(bloc.clone());
        stats.record_base_destroyed(bloc.clone(), DestructionSource::Combat);
        stats.record_base_destroyed(bloc.clone(), DestructionSource::CoordinationService);
        stats.record_base_destroyed(bloc.clone(), DestructionSource::AuthorizedUser);
        stats.record_trust_destroyed(bloc.clone(), DestructionSource::Combat);
        stats.record_trust_destroyed(bloc.clone(), DestructionSource::CoordinationService);
        stats.record_trust_destroyed(bloc.clone(), DestructionSource::AuthorizedUser);

        let bloc_stats = stats.bloc(&bloc);
        assert_eq!(bloc_stats.bases().built(), 1);
        assert_eq!(bloc_stats.bases().destroyed_in_combat(), 1);
        assert_eq!(bloc_stats.bases().destroyed_via_coordination_service(), 1);
        assert_eq!(bloc_stats.bases().destroyed_by_authorized_users(), 1);
        assert_eq!(bloc_stats.trusts().built(), 1);
        assert_eq!(bloc_stats.trusts().destroyed_in_combat(), 1);
        assert_eq!(bloc_stats.trusts().destroyed_via_coordination_service(), 1);
        assert_eq!(bloc_stats.trusts().destroyed_by_authorized_users(), 1);
        assert_eq!(bloc_stats.units().produced(), 3);
        assert_eq!(bloc_stats.units().destroyed_by_enemies(), 1);
    }

    #[test]
    fn persisted_round_trip_preserves_counters_and_start_time() {
        let bloc = BlocKey::from("bloc-a".to_string());
        let mut stats = SimulationStats::default();
        stats.record_base_built(bloc);

        let document = mongodb::bson::to_document(&stats).unwrap();
        let restored: SimulationStats = mongodb::bson::from_document(document).unwrap();

        assert_eq!(restored, stats);
    }
}
