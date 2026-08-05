use std::sync::Arc;

use derive_more::Display;
use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    domain::{Loot, LootFactors, MilitaryBase, politics::DieRollOutcome},
    geometry::{Distance, Point, Positioned, WorldBounds},
    services::credit_exchange_service::{Cost, Payment, SinglePayer},
};

/// Identifies a [MilitaryUnit].
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    utoipa::ToSchema,
    derive_more::From,
    derive_more::Into,
    Display,
)]
pub(crate) struct UnitId(Uuid);

impl UnitId {
    fn new() -> Self {
        let id = Uuid::new();
        Self(id)
    }
}

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnit {
    id: UnitId,
    base: Arc<RwLock<MilitaryBase>>,
    position: Point,
    state: UnitState,
    /// The loot that will be collected if this unit is killed.
    loot: Loot,
}

#[derive(Debug, Default, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state")]
pub(crate) enum UnitState {
    /// The unit is alive and well
    #[default]
    Alive,
    /// The unit was killed
    Killed { by: UnitId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttackOutcome {
    Miss,
    Killed,
}

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(
        payment: Payment<Self, SinglePayer>,
        loot_factors: &LootFactors,
        base: Arc<RwLock<MilitaryBase>>,
        position: Point,
    ) -> Self {
        let loot = Loot::from_cost(payment.cost(), loot_factors);
        Self::new_prepaid_with_loot(base, position, loot)
    }

    /// Create a unit whose configured cost has already been paid.
    pub(crate) fn new_prepaid(
        cost: &Cost<Self>,
        loot_factors: &LootFactors,
        base: Arc<RwLock<MilitaryBase>>,
        position: Point,
    ) -> Self {
        let loot = Loot::from_cost(cost, loot_factors);
        Self::new_prepaid_with_loot(base, position, loot)
    }

    fn new_prepaid_with_loot(base: Arc<RwLock<MilitaryBase>>, position: Point, loot: Loot) -> Self {
        Self {
            id: UnitId::new(),
            base,
            position,
            state: Default::default(),
            loot,
        }
    }

    pub(crate) fn id(&self) -> UnitId {
        self.id
    }

    pub(crate) fn state(&self) -> UnitState {
        self.state
    }

    pub(crate) fn loot(&self) -> &Loot {
        &self.loot
    }

    pub(crate) fn kill(&mut self, by: UnitId) {
        self.state = UnitState::Killed { by };
    }

    pub(crate) fn from_persisted(
        id: Uuid,
        base: Arc<RwLock<MilitaryBase>>,
        position: Point,
        state: UnitState,
        loot: Loot,
    ) -> Self {
        Self {
            id: id.into(),
            base,
            position,
            state,
            loot,
        }
    }

    /// Acquires a read lock on the unit's base and returns the guard.
    pub(crate) async fn base(&self) -> RwLockReadGuard<'_, MilitaryBase> {
        self.base.read().await
    }

    pub(crate) fn rebase(&mut self, base: Arc<RwLock<MilitaryBase>>) {
        self.base = base;
    }

    #[expect(dead_code)]
    pub(crate) fn set_position(&mut self, position: Point) {
        self.position = position;
    }

    /// Moves the unit `step` distance toward `target`, snapping to it if closer.
    pub(crate) fn move_toward(&mut self, target: Point, step: Distance, world_bounds: WorldBounds) {
        self.position = world_bounds.step_toward(self.position, target, step);
    }

    /// Roll the [Chance][crate::domain::politics::Chance]-sided die for this unit's attack.
    ///
    /// This does not mutate the target immediately. Combat resolution applies deaths after all units that were alive at
    /// the beginning of the tick have attacked.
    pub(crate) async fn attack(&self) -> AttackOutcome {
        let base = self.base().await;
        let bloc = base.bloc().await;
        let roll = bloc.chance().roll();
        if roll == DieRollOutcome::Hit {
            return AttackOutcome::Killed;
        }
        AttackOutcome::Miss
    }
}
