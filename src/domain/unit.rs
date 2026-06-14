use std::sync::Arc;

use mongodb::bson::Uuid;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    domain::{MilitaryBase, politics::DieRollOutcome},
    geometry::{Distance, Point, Positioned},
    services::payment_service::{Payment, SinglePayer},
};

/// Identifies a [MilitaryUnit].
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

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(_payment: Payment<Self, SinglePayer>, base: Arc<RwLock<MilitaryBase>>, position: Point) -> Self {
        Self {
            id: UnitId::new(),
            base,
            position,
            state: Default::default(),
        }
    }

    pub(crate) fn id(&self) -> UnitId {
        self.id
    }

    pub(crate) fn state(&self) -> UnitState {
        self.state
    }

    pub(crate) fn was_killed_by(&self, other: UnitId) -> bool {
        self.state() == UnitState::Killed { by: other }
    }

    pub(crate) fn from_persisted(id: Uuid, base: Arc<RwLock<MilitaryBase>>, position: Point, state: UnitState) -> Self {
        Self {
            id: id.into(),
            base,
            position,
            state,
        }
    }

    /// Acquires a read lock on the unit's base and returns the guard.
    pub(crate) async fn base(&self) -> RwLockReadGuard<'_, MilitaryBase> {
        self.base.read().await
    }

    #[expect(dead_code)]
    pub(crate) fn set_position(&mut self, position: Point) {
        self.position = position;
    }

    /// Moves the unit `step` distance toward `target`, snapping to it if closer.
    pub(crate) fn move_toward(&mut self, target: Point, step: Distance) {
        let from = self.position;
        let diff = target - from;
        let dist = from.distance_to(&target);
        self.position = if dist <= step {
            target
        } else {
            let scale = step / dist;
            from + diff * scale
        };
    }

    /// Roll the [Chance][crate::domain::politics::Chance]-sided die, and kill the other unit if it's a
    /// [hit][crate::domain::politics::DieRollOutcome::Hit].
    pub(crate) async fn attack(&self, other: &mut Self) {
        // Can't kill a unit twice!
        if other.state() != UnitState::Alive {
            return;
        }
        let base = self.base().await;
        let roll = base.bloc().chance().roll();
        if roll == DieRollOutcome::Hit {
            other.state = UnitState::Killed { by: self.id }
        }
    }
}
