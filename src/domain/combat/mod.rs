mod structure;

use std::{collections::HashMap, sync::Arc};

use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    domain::{
        BaseId, BlocName, MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId, combat::structure::CombatStructure,
    },
    geometry::{Point, Positioned},
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct UnitKilled {
    killer: UnitId,
    killed: UnitId,
}

/// What may happen during a combat tick?
#[derive(Debug)]
pub(crate) enum CombatEvent {
    /// Nothing happened. This can be the case when
    /// - units in combat rolled the dice and survived
    /// - units are attacking a structure but are too few to destroy it
    None,
    UnitsKilled(Vec<UnitKilled>),
    /// The trust was destroyed, this implies combat end.
    TrustDestroyed(TrustId),
    /// The base was destroyed, this implies combat end.
    BaseDestroyed(BaseId),
}

/// These are the possible initial states of a combat
pub(crate) enum CombatParameters {
    /// Just units in a combat
    Units(HashMap<BlocName, Vec<Arc<RwLock<MilitaryUnit>>>>),
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
    units: HashMap<BlocName, Vec<Arc<RwLock<MilitaryUnit>>>>,
    structure: CombatStructure,
    state: CombatState,
}

impl Combat {
    pub(crate) async fn new(params: CombatParameters) -> Self {
        let (units, structure) = match params {
            CombatParameters::Units(units) => {
                // assert implementation correctness
                assert!(
                    units.len() >= 2,
                    "In a unit-only combat, there must be units of at least 2 blocs."
                );
                for bloc_units in units.values() {
                    assert!(!bloc_units.is_empty(), "In each bloc, there must be at least 1 unit.");
                }
                (units, CombatStructure::None)
            }
            CombatParameters::Trust(unit, trust, threshold) => {
                let bloc = unit_bloc_name(&unit).await;
                (
                    HashMap::from([(bloc, vec![unit])]),
                    CombatStructure::Trust {
                        trust: trust.clone(),
                        destruction_threshold: threshold,
                    },
                )
            }
            CombatParameters::Base(unit, base, threshold) => {
                let bloc = unit_bloc_name(&unit).await;
                (
                    HashMap::from([(bloc, vec![unit])]),
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
            .first()
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

    pub(crate) fn from_persisted(
        id: Uuid,
        position: Point,
        units: HashMap<BlocName, Vec<Arc<RwLock<MilitaryUnit>>>>,
        structure: CombatStructureParameters,
        state: CombatState,
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
        }
    }

    pub(crate) async fn unit_ids_by_bloc(&self) -> Vec<(BlocName, Vec<UnitId>)> {
        let mut result = Vec::with_capacity(self.units.len());
        for (bloc, units) in &self.units {
            let mut unit_ids = Vec::with_capacity(units.len());
            for unit in units {
                unit_ids.push(unit.read().await.id());
            }
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

    /// Merge this combat with another existing combat
    pub(crate) fn merge(&mut self, other: Self) {
        assert_eq!(
            self.position, other.position,
            "merged combats must be in the same position"
        );
    }

    /// Let all blocs of this combat attack each other with their units, or, if there is just a single bloc
    /// present in this combat and a structure, destroy the structure, if applicable.
    pub(crate) async fn tick(&mut self) -> CombatEvent {
        // There is only units of a single bloc and the combat is still running, ...
        if self.units.len() == 1 {
            // ... this means we're in a situation where we're waiting for more units of the same bloc to arrive before
            // the threshold is reached to destroy the structure.
            self.check_for_structure_destruction().await
        } else {
            self.units_fight().await
        }
    }

    async fn check_for_structure_destruction(&mut self) -> CombatEvent {
        #[cfg(debug_assertions)]
        {
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
            CombatStructure::Trust { trust, .. } => CombatEvent::TrustDestroyed(trust.read().await.id()),
            CombatStructure::Base { base, .. } => CombatEvent::BaseDestroyed(base.read().await.id()),
        }
    }

    async fn units_fight(&mut self) -> CombatEvent {
        let mut killed_events = Vec::new();
        // Pointers to the killed units
        let mut killed_units = Vec::new();

        for (bloc_a, units_a) in self.units.iter() {
            for (bloc_b, units_b) in self.units.iter() {
                if bloc_a == bloc_b {
                    continue;
                }

                for unit_a in units_a {
                    for unit_b in units_b {
                        let unit_a_guard = unit_a.read().await;
                        let mut unit_b_guard = unit_b.write().await;

                        unit_a_guard.attack(&mut unit_b_guard).await;

                        if unit_b_guard.was_killed_by(unit_a_guard.id()) {
                            killed_events.push(UnitKilled {
                                killed: unit_b_guard.id(),
                                killer: unit_a_guard.id(),
                            });

                            killed_units.push(Arc::clone(unit_b));
                        }
                    }
                }
            }
        }

        if killed_units.is_empty() {
            return CombatEvent::None;
        }

        // Remove killed units from combat.
        for units in self.units.values_mut() {
            units.retain(|unit| !killed_units.iter().any(|killed| Arc::ptr_eq(unit, killed)));
        }

        // Remove blocs that have no units left.
        self.units.retain(|_, units| !units.is_empty());

        // If we have only units of a single bloc and there is no structure, the combat has ended.
        let combat_end = self.units.len() == 1 && self.structure.destruction_threshold().is_none();
        if combat_end {
            self.state = CombatState::Ended
        }

        CombatEvent::UnitsKilled(killed_events)
    }
}

async fn unit_bloc_name(unit_a: &RwLock<MilitaryUnit>) -> BlocName {
    let military_unit = unit_a.read().await;
    let military_base = military_unit.base().await;
    military_base.bloc().name().clone()
}
