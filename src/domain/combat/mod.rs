pub(super) mod loot;
mod structure;

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    sync::Arc,
};

use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    domain::{
        AttackOutcome, BaseId, BlocKey, Loot, MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId, UnitState,
        combat::structure::CombatStructure,
    },
    geometry::{Point, Positioned},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct LootTransfer {
    base_id: BaseId,
    loot: Loot,
}

impl LootTransfer {
    pub(crate) fn base_id(&self) -> BaseId {
        self.base_id
    }

    pub(crate) fn loot(&self) -> &Loot {
        &self.loot
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct UnitKilled {
    killer: UnitId,
    killed: UnitId,
    killed_bloc: BlocKey,
    /// The loot transferred to the killer's base
    loot: LootTransfer,
}

impl UnitKilled {
    pub(crate) fn killer(&self) -> UnitId {
        self.killer
    }

    pub(crate) fn killed(&self) -> UnitId {
        self.killed
    }

    pub(crate) fn killed_bloc(&self) -> &BlocKey {
        &self.killed_bloc
    }

    pub(crate) fn loot(&self) -> &LootTransfer {
        &self.loot
    }
}

/// What may happen during a combat tick?
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(crate) enum CombatEvent {
    /// Nothing happened. This can be the case when
    /// - units in combat rolled the dice and survived
    /// - units are attacking a structure but are too few to destroy it
    None,
    UnitsKilled {
        units: Vec<UnitKilled>,
    },
    /// The trust was destroyed, this implies combat end.
    TrustDestroyed {
        id: TrustId,
        #[serde(default)]
        loot: Vec<LootTransfer>,
    },
    /// The base was destroyed, this implies combat end.
    BaseDestroyed {
        id: BaseId,
        #[serde(default)]
        loot: Vec<LootTransfer>,
    },
}

impl CombatEvent {
    fn should_persist(&self) -> bool {
        match self {
            Self::None => false,
            Self::UnitsKilled { units } => !units.is_empty(),
            Self::TrustDestroyed { .. } | Self::BaseDestroyed { .. } => true,
        }
    }
}

/// These are the possible initial states of a combat
pub(crate) enum CombatParameters {
    /// Just units in a combat
    Units(HashMap<BlocKey, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>),
    /// A trust is attacked
    Trust(Arc<RwLock<MilitaryUnit>>, Arc<RwLock<Trust>>, u32),
    /// A base is attacked
    Base(Arc<RwLock<MilitaryUnit>>, Arc<RwLock<MilitaryBase>>, u32),
}

pub(crate) enum CombatStructureParameters {
    None,
    Trust(Arc<RwLock<Trust>>, u32),
    Base(Arc<RwLock<MilitaryBase>>, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CombatStructureSnapshot {
    None,
    Trust { id: TrustId, destruction_threshold: u32 },
    Base { id: BaseId, destruction_threshold: u32 },
}

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, utoipa::ToSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CombatState {
    #[default]
    Ongoing,
    /// The combat has ended and may be dropped.
    /// This is the case when there are only units of a single bloc left.
    Ended,
}

/// Identifies a [Combat].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
    derive_more::From,
    derive_more::Into,
)]
pub(crate) struct CombatId(Uuid);

pub(crate) struct Combat {
    id: CombatId,
    position: Point,
    units: HashMap<BlocKey, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>,
    structure: CombatStructure,
    state: CombatState,
    events: Vec<CombatEvent>,
}

impl Combat {
    pub(crate) async fn new(params: CombatParameters) -> Self {
        let (units, structure) = match params {
            CombatParameters::Units(units) => {
                // assert implementation correctness
                assert!(
                    units.len() >= 2,
                    "In a unit-only combat, there must be units of at least 2 blocs. Instead, had {units:?}"
                );
                for bloc_units in units.values() {
                    assert!(!bloc_units.is_empty(), "In each bloc, there must be at least 1 unit.");
                }
                (units, CombatStructure::None)
            }
            CombatParameters::Trust(unit, trust, threshold) => {
                let bloc = unit_bloc_name(&unit).await;
                let id = unit.read().await.id();
                let units = HashMap::from([(id, unit.clone())]);
                (
                    HashMap::from([(bloc, units)]),
                    CombatStructure::Trust {
                        trust: trust.clone(),
                        destruction_threshold: threshold,
                    },
                )
            }
            CombatParameters::Base(unit, base, threshold) => {
                let bloc = unit_bloc_name(&unit).await;
                let id = unit.read().await.id();
                let units = HashMap::from([(id, unit.clone())]);
                (
                    HashMap::from([(bloc, units)]),
                    CombatStructure::Base {
                        base: base.clone(),
                        destruction_threshold: threshold,
                    },
                )
            }
        };

        let position = units
            .values()
            .next()
            .expect("we just created the map with at least 1 bloc")
            .values()
            .next()
            .expect("we just created the unit list with at least 1 unit")
            .read()
            .await
            .position();

        Self {
            id: Uuid::new().into(),
            units,
            position,
            structure,
            state: Default::default(),
            events: Vec::new(),
        }
    }

    pub(crate) fn position(&self) -> Point {
        self.position
    }

    pub(crate) fn id(&self) -> CombatId {
        self.id
    }

    pub(crate) fn state(&self) -> CombatState {
        self.state
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub(crate) fn events(&self) -> &[CombatEvent] {
        &self.events
    }

    pub(crate) fn from_persisted(
        id: Uuid,
        position: Point,
        units: HashMap<BlocKey, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>,
        structure: CombatStructureParameters,
        state: CombatState,
        events: Vec<CombatEvent>,
    ) -> Self {
        let structure = match structure {
            CombatStructureParameters::None => CombatStructure::None,
            CombatStructureParameters::Trust(trust, destruction_threshold) => CombatStructure::Trust {
                trust,
                destruction_threshold,
            },
            CombatStructureParameters::Base(base, destruction_threshold) => CombatStructure::Base {
                base,
                destruction_threshold,
            },
        };

        Self {
            id: id.into(),
            position,
            units,
            structure,
            state,
            events,
        }
    }

    pub(crate) async fn unit_ids_by_bloc(&self) -> Vec<(BlocKey, Vec<UnitId>)> {
        let mut result = Vec::with_capacity(self.units.len());
        for (bloc, units) in &self.units {
            let unit_ids = units.keys().copied().collect();
            result.push((bloc.clone(), unit_ids));
        }
        result
    }

    pub(crate) async fn structure_snapshot(&self) -> CombatStructureSnapshot {
        match &self.structure {
            CombatStructure::None => CombatStructureSnapshot::None,
            CombatStructure::Trust {
                trust,
                destruction_threshold,
            } => CombatStructureSnapshot::Trust {
                id: trust.read().await.id(),
                destruction_threshold: *destruction_threshold,
            },
            CombatStructure::Base {
                base,
                destruction_threshold,
            } => CombatStructureSnapshot::Base {
                id: base.read().await.id(),
                destruction_threshold: *destruction_threshold,
            },
        }
    }

    pub(crate) async fn include_unit(&mut self, unit: Arc<RwLock<MilitaryUnit>>) -> bool {
        let bloc = unit_bloc_name(&unit).await;
        let id = unit.read().await.id();
        self.units.entry(bloc).or_default().insert(id, unit).is_none()
    }

    /// Let all blocs of this combat attack each other with their units, or, if there is just a single bloc
    /// present in this combat and a structure, destroy the structure, if applicable.
    pub(crate) async fn tick(&mut self) -> CombatEvent {
        self.prune_dead_units().await;
        if self.state == CombatState::Ended {
            return CombatEvent::None;
        }

        // There is only units of a single bloc and the combat is still running, ...
        let event = if self.units.len() == 1 {
            // ... this means we're in a situation where we're waiting for more units of the same bloc to arrive before
            // the threshold is reached to destroy the structure.
            self.check_for_structure_destruction().await
        } else {
            self.units_fight().await
        };

        if event.should_persist() {
            self.events.push(event.clone());
        }

        event
    }

    async fn prune_dead_units(&mut self) {
        let mut dead_units = HashSet::new();
        for units in self.units.values() {
            for (unit_id, unit) in units {
                if unit.read().await.state() != UnitState::Alive {
                    dead_units.insert(*unit_id);
                }
            }
        }

        if !dead_units.is_empty() {
            for units in self.units.values_mut() {
                units.retain(|unit_id, _| !dead_units.contains(unit_id));
            }
            self.units.retain(|_, units| !units.is_empty());
        }

        self.end_if_finished();
    }

    fn end_if_finished(&mut self) {
        let combat_end =
            self.units.is_empty() || self.units.len() == 1 && self.structure.destruction_threshold().is_none();
        if combat_end && self.state != CombatState::Ended {
            self.state = CombatState::Ended;
            log::debug!("combat {:?} at position {:?} has ended", self.id, self.position)
        }
    }

    async fn check_for_structure_destruction(&mut self) -> CombatEvent {
        let structure_bloc = self
            .structure
            .bloc()
            .await
            .expect("we should be in a combat with a structure");

        if structure_bloc == *self.units.keys().next().expect("we have exactly 1 bloc") {
            log::warn!(
                "combat {:?} at position {:?} only contains units of bloc {structure_bloc:?} attacking their own structure, ending it",
                self.id,
                self.position
            );
            self.state = CombatState::Ended;
            return CombatEvent::None;
        }

        let destruction_threshold = self
            .structure
            .destruction_threshold()
            .expect("we should be in a combat with a structure");

        let units_count = self
            .units
            .values()
            .next()
            .expect("We just checked for length 1 above")
            .len();

        if units_count < (destruction_threshold as usize) {
            return CombatEvent::None;
        }

        // If we're still here, the structure was destroyed!
        self.state = CombatState::Ended;
        match &self.structure {
            CombatStructure::None => panic!("We shouldn't be reaching this point in a combat without structures"),
            CombatStructure::Trust { trust, .. } => {
                let trust = trust.read().await;
                CombatEvent::TrustDestroyed {
                    id: trust.id(),
                    loot: self.structure_loot_transfers(trust.loot()).await,
                }
            }
            CombatStructure::Base { base, .. } => {
                let base = base.read().await;
                CombatEvent::BaseDestroyed {
                    id: base.id(),
                    loot: self.structure_loot_transfers(base.loot()).await,
                }
            }
        }
    }

    /// A destroyed structure results in multiple loot transfers because the participating units may come from different
    /// bases.
    async fn structure_loot_transfers(&self, loot: &Loot) -> Vec<LootTransfer> {
        let units = self
            .units
            .values()
            .next()
            .expect("structure destruction requires exactly one attacking bloc");
        // We need to split the loot because the units could be coming from different bases.
        let split_loot = loot.split(units.len());
        let mut transfers = Vec::with_capacity(units.len());
        for unit in units.values() {
            let base_id = unit.read().await.base().await.id();
            transfers.push(LootTransfer {
                base_id,
                loot: split_loot.clone(),
            });
        }
        transfers
    }

    async fn units_fight(&mut self) -> CombatEvent {
        let mut killed_events = Vec::new();
        let mut killed_units = HashMap::new();
        let mut alive_units = HashMap::new();
        let mut available_targets = HashMap::<BlocKey, BTreeSet<UnitId>>::new();

        for (bloc, units) in &self.units {
            for (unit_id, unit) in units {
                if unit.read().await.state() == UnitState::Alive {
                    alive_units.insert(*unit_id, (bloc.clone(), unit.clone()));
                    available_targets.entry(bloc.clone()).or_default().insert(*unit_id);
                }
            }
        }

        let mut attacker_ids = alive_units.keys().copied().collect::<Vec<_>>();
        attacker_ids.sort_unstable();
        for attacker_id in attacker_ids {
            let (attacker_bloc, attacker) = alive_units
                .get(&attacker_id)
                .expect("attacker ids were collected from alive units");
            let Some(target_id) = available_targets
                .iter()
                .filter(|(target_bloc, _)| *target_bloc != attacker_bloc)
                .filter_map(|(_, targets)| targets.first())
                .min()
                .copied()
            else {
                continue;
            };
            let (target_bloc, target) = alive_units
                .get(&target_id)
                .expect("available target ids belong to alive units");

            let attacker = attacker.read().await;

            if attacker.attack().await == AttackOutcome::Killed && killed_units.insert(target_id, attacker_id).is_none()
            {
                available_targets
                    .get_mut(target_bloc)
                    .expect("the target bloc has an available-target set")
                    .remove(&target_id);
                let killer_base = attacker.base().await.id();
                let target = target.read().await;
                let loot = target.loot().clone();
                killed_events.push(UnitKilled {
                    killed: target_id,
                    killer: attacker_id,
                    killed_bloc: target_bloc.clone(),
                    loot: LootTransfer {
                        base_id: killer_base,
                        loot,
                    },
                });
            }
        }

        if killed_units.is_empty() {
            return CombatEvent::None;
        }

        // Resolve deaths after all units that were alive at the start of the tick attacked.
        for (unit_id, killer_id) in &killed_units {
            let (_, unit) = alive_units
                .get(unit_id)
                .expect("killed unit ids belong to the alive-at-start snapshot");
            unit.write().await.kill(*killer_id);
        }

        self.prune_dead_units().await;

        CombatEvent::UnitsKilled { units: killed_events }
    }
}

async fn unit_bloc_name(unit_a: &RwLock<MilitaryUnit>) -> BlocKey {
    let military_unit = unit_a.read().await;
    let military_base = military_unit.base().await;
    military_base.bloc_key().clone()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::Uuid;
    use ordered_float::NotNan;

    use super::*;
    use crate::{
        domain::{Bloc, BlocName, Chance, LootFactors, Placement, PlacementId, Zone, ZoneKey, ZoneName},
        services::credit_exchange_service::{Cost, Share},
    };

    fn base(bloc: &str, position: Point) -> Arc<RwLock<MilitaryBase>> {
        let bloc_key = BlocKey::from(bloc.to_owned());
        let bloc_name = BlocName::from(bloc.to_owned());
        let bloc_state = Arc::new(RwLock::new(Bloc::new(
            bloc_key.clone(),
            bloc_name.clone(),
            Chance::new(1),
            Share::default(),
        )));
        let zone = Arc::new(Zone::new_with_social_rules(
            ZoneKey::from(format!("{bloc}-zone")),
            ZoneName::from(format!("{bloc} zone")),
            bloc_key,
            bloc_name,
            bloc_state,
            Vec::new(),
        ));
        let placement = Arc::new(Placement::new(
            serde_json::from_value::<PlacementId>(serde_json::json!(format!("{bloc}-placement"))).unwrap(),
            zone,
            position,
        ));
        let cost: Cost<MilitaryBase> = serde_json::from_value(serde_json::json!({
            "money": 0.0,
            "resources": {}
        }))
        .unwrap();

        Arc::new(RwLock::new(MilitaryBase::new_prepaid(
            Vec::new(),
            &cost,
            &LootFactors::default(),
            placement,
        )))
    }

    fn unit(base: Arc<RwLock<MilitaryBase>>, position: Point) -> Arc<RwLock<MilitaryUnit>> {
        unit_with_uuid(base, position, Uuid::new())
    }

    fn unit_with_id(base: Arc<RwLock<MilitaryBase>>, position: Point, id: u8) -> Arc<RwLock<MilitaryUnit>> {
        unit_with_uuid(base, position, Uuid::from_bytes([id; 16]))
    }

    fn unit_with_uuid(base: Arc<RwLock<MilitaryBase>>, position: Point, id: Uuid) -> Arc<RwLock<MilitaryUnit>> {
        Arc::new(RwLock::new(MilitaryUnit::from_persisted(
            id,
            base,
            position,
            UnitState::Alive,
            Loot::default(),
        )))
    }

    async fn units_by_id(
        units: impl IntoIterator<Item = Arc<RwLock<MilitaryUnit>>>,
    ) -> HashMap<UnitId, Arc<RwLock<MilitaryUnit>>> {
        let mut result = HashMap::new();
        for unit in units {
            let id = unit.read().await.id();
            result.insert(id, unit);
        }
        result
    }

    #[tokio::test]
    async fn attacks_target_lowest_available_enemy_unit_id() {
        let position = Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap());
        let bloc_a_base = base("bloc-a", position);
        let bloc_b_base = base("bloc-b", position);
        let a1 = unit_with_id(bloc_a_base.clone(), position, 1);
        let b2 = unit_with_id(bloc_b_base.clone(), position, 2);
        let b3 = unit_with_id(bloc_b_base, position, 3);
        let a4 = unit_with_id(bloc_a_base, position, 4);
        let a1_id = a1.read().await.id();
        let b2_id = b2.read().await.id();
        let b3_id = b3.read().await.id();
        let a4_id = a4.read().await.id();
        let units = HashMap::from([
            (BlocKey::from("bloc-a".to_owned()), units_by_id([a4, a1]).await),
            (BlocKey::from("bloc-b".to_owned()), units_by_id([b3, b2]).await),
        ]);
        let mut combat = Combat::new(CombatParameters::Units(units)).await;

        let CombatEvent::UnitsKilled { units } = combat.tick().await else {
            panic!("guaranteed hits should kill units");
        };

        let attacks = units
            .iter()
            .map(|event| (event.killer(), event.killed()))
            .collect::<Vec<_>>();
        assert_eq!(
            attacks,
            vec![(a1_id, b2_id), (b2_id, a1_id), (b3_id, a4_id), (a4_id, b3_id)]
        );
    }

    #[tokio::test]
    async fn combat_tick_handles_eight_thousand_units() {
        let position = Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap());
        let bloc_a_base = base("bloc-a", position);
        let bloc_b_base = base("bloc-b", position);
        let mut bloc_a_units = Vec::with_capacity(4_000);
        let mut bloc_b_units = Vec::with_capacity(4_000);
        for _ in 0..4_000 {
            bloc_a_units.push(unit(bloc_a_base.clone(), position));
            bloc_b_units.push(unit(bloc_b_base.clone(), position));
        }
        let units = HashMap::from([
            (BlocKey::from("bloc-a".to_owned()), units_by_id(bloc_a_units).await),
            (BlocKey::from("bloc-b".to_owned()), units_by_id(bloc_b_units).await),
        ]);
        let mut combat = Combat::new(CombatParameters::Units(units)).await;

        let event = tokio::time::timeout(std::time::Duration::from_secs(10), combat.tick())
            .await
            .expect("an 8,000-unit combat tick should complete promptly");

        assert!(matches!(event, CombatEvent::UnitsKilled { units } if units.len() == 8_000));
    }

    #[tokio::test]
    async fn each_unit_attacks_at_most_once_per_tick() {
        let position = Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap());
        let attacker_base = base("attackers", position);
        let defender_base = base("defenders", position);
        let attacker = unit(attacker_base, position);
        let defender_a = unit(defender_base.clone(), position);
        let defender_b = unit(defender_base, position);
        let attacker_id = attacker.read().await.id();
        let defender_a_id = defender_a.read().await.id();
        let defender_b_id = defender_b.read().await.id();
        let units = HashMap::from([
            (
                BlocKey::from("attackers".to_owned()),
                HashMap::from([(attacker_id, attacker)]),
            ),
            (
                BlocKey::from("defenders".to_owned()),
                HashMap::from([(defender_a_id, defender_a.clone()), (defender_b_id, defender_b.clone())]),
            ),
        ]);
        let mut combat = Combat::new(CombatParameters::Units(units)).await;

        let event = combat.tick().await;

        let CombatEvent::UnitsKilled { units } = event else {
            panic!("guaranteed hits should kill units");
        };
        assert_eq!(units.len(), 2);
        let surviving_defenders = usize::from(defender_a.read().await.state() == UnitState::Alive)
            + usize::from(defender_b.read().await.state() == UnitState::Alive);
        assert_eq!(surviving_defenders, 1);
    }

    #[tokio::test]
    async fn attacks_skip_units_already_killed_this_tick() {
        let position = Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap());
        let bloc_a_base = base("bloc-a", position);
        let bloc_b_base = base("bloc-b", position);
        let bloc_a_units = [unit(bloc_a_base.clone(), position), unit(bloc_a_base, position)];
        let bloc_b_units = [unit(bloc_b_base.clone(), position), unit(bloc_b_base, position)];
        let units = HashMap::from([
            (
                BlocKey::from("bloc-a".to_owned()),
                HashMap::from([
                    (bloc_a_units[0].read().await.id(), bloc_a_units[0].clone()),
                    (bloc_a_units[1].read().await.id(), bloc_a_units[1].clone()),
                ]),
            ),
            (
                BlocKey::from("bloc-b".to_owned()),
                HashMap::from([
                    (bloc_b_units[0].read().await.id(), bloc_b_units[0].clone()),
                    (bloc_b_units[1].read().await.id(), bloc_b_units[1].clone()),
                ]),
            ),
        ]);
        let mut combat = Combat::new(CombatParameters::Units(units)).await;

        let event = combat.tick().await;

        let CombatEvent::UnitsKilled { units } = event else {
            panic!("guaranteed hits should kill units");
        };
        assert_eq!(units.len(), 4);
        assert_eq!(units.iter().map(UnitKilled::killed).collect::<HashSet<_>>().len(), 4);
        assert_eq!(units.iter().map(UnitKilled::killer).collect::<HashSet<_>>().len(), 4);
    }

    #[tokio::test]
    async fn combat_ends_when_only_the_structure_blocs_own_units_remain() {
        let position = Point::new(NotNan::new(0.0).unwrap(), NotNan::new(0.0).unwrap());
        let bloc_a_base = base("bloc-a", position);
        let attacker = unit(bloc_a_base.clone(), position);
        let mut combat = Combat::new(CombatParameters::Base(attacker, bloc_a_base, 1)).await;

        let event = combat.tick().await;

        assert!(matches!(event, CombatEvent::None));
        assert_eq!(combat.state(), CombatState::Ended);
    }
}
