use mongodb::bson::Uuid;
use serde::Serialize;

use crate::{
    domain::BaseId,
    geometry::{Point, Positioned},
    services::payment_service::{Payment, SinglePayer},
};

/// Identifies a [MilitaryUnit].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, derive_more::From, derive_more::Into)]
pub(crate) struct UnitId(String);

impl UnitId {
    fn new() -> Self {
        let id = Uuid::new();
        Self(id.to_string())
    }
}

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MilitaryUnit {
    id: UnitId,
    base_id: BaseId,
    position: Point,
}

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(_payment: Payment<Self, SinglePayer>, base_id: BaseId, position: Point) -> Self {
        Self {
            id: UnitId::new(),
            base_id,
            position,
        }
    }

    pub(crate) fn id(&self) -> &UnitId {
        &self.id
    }

    pub(crate) fn from_persisted(id: String, base_id: BaseId, position: Point) -> Self {
        Self {
            id: id.into(),
            base_id,
            position,
        }
    }

    pub(crate) fn base_id(&self) -> BaseId {
        self.base_id
    }
}
