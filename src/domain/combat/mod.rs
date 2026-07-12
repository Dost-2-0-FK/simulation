pub(super) mod loot;
mod structure;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    domain::{
        AttackOutcome, BaseId, BlocName, Loot, MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId, UnitState,
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
    Units(HashMap<BlocName, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>),
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
    units: HashMap<BlocName, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>,
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
        units: HashMap<BlocName, HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>>,
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

    pub(crate) async fn unit_ids_by_bloc(&self) -> Vec<(BlocName, Vec<UnitId>)> {
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

        assert_ne!(
            structure_bloc,
            self.units.keys().next().expect("we have exactly 1 bloc").clone(),
            "we cannot be in a state where units of a bloc attack their own structure"
        );

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
        let mut alive_units = Vec::new();

        for (bloc, units) in &self.units {
            for (unit_id, unit) in units {
                if unit.read().await.state() == UnitState::Alive {
                    alive_units.push((bloc.clone(), *unit_id, unit.clone()));
                }
            }
        }

        for (bloc_a, unit_a_id, unit_a) in &alive_units {
            for (bloc_b, unit_b_id, unit_b) in &alive_units {
                if bloc_a == bloc_b {
                    continue;
                }

                let unit_a_guard = unit_a.read().await;

                if unit_a_guard.attack().await == AttackOutcome::Killed
                    && killed_units.insert(*unit_b_id, *unit_a_id).is_none()
                {
                    let killer_base = unit_a_guard.base().await.id();
                    let unit_b_guard = unit_b.read().await;
                    let loot = unit_b_guard.loot().clone();
                    log::debug!(
                        "unit {killer} ({bloc_a}) killed unit {killed} ({bloc_b}).",
                        killer = unit_a_id,
                        killed = unit_b_id,
                    );
                    killed_events.push(UnitKilled {
                        killed: *unit_b_id,
                        killer: *unit_a_id,
                        loot: LootTransfer {
                            base_id: killer_base,
                            loot,
                        },
                    });
                }
            }
        }

        if killed_units.is_empty() {
            return CombatEvent::None;
        }

        // Resolve deaths after all units that were alive at the start of the tick attacked.
        for (unit_id, killer_id) in &killed_units {
            if let Some(unit) = alive_units
                .iter()
                .find_map(|(_, candidate_id, unit)| (candidate_id == unit_id).then_some(unit))
            {
                unit.write().await.kill(*killer_id);
            }
        }

        self.prune_dead_units().await;

        CombatEvent::UnitsKilled { units: killed_events }
    }
}

async fn unit_bloc_name(unit_a: &RwLock<MilitaryUnit>) -> BlocName {
    let military_unit = unit_a.read().await;
    let military_base = military_unit.base().await;
    military_base.bloc_name().clone()
}
