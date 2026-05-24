use std::sync::Arc;

use mongodb::bson::Uuid;
use serde::Serialize;
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    domain::MilitaryBase,
    geometry::{Distance, Point, Positioned},
    services::payment_service::{Payment, SinglePayer},
};

/// Identifies a [MilitaryUnit].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, utoipa::ToSchema, derive_more::From, derive_more::Into)]
pub(crate) struct UnitId(String);

impl UnitId {
    fn new() -> Self {
        let id = Uuid::new();
        Self(id.to_string())
    }
}

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnit {
    id: UnitId,
    base: Arc<RwLock<MilitaryBase>>,
    position: Point,
}

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(_payment: Payment<Self, SinglePayer>, base: Arc<RwLock<MilitaryBase>>, position: Point) -> Self {
        Self {
            id: UnitId::new(),
            base,
            position,
        }
    }

    pub(crate) fn id(&self) -> &UnitId {
        &self.id
    }

    pub(crate) fn from_persisted(id: String, base: Arc<RwLock<MilitaryBase>>, position: Point) -> Self {
        Self {
            id: id.into(),
            base,
            position,
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
}
