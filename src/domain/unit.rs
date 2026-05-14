use serde::Serialize;

use crate::{
    domain::BaseId,
    geometry::{Point, Positioned},
    services::payment_service::{Payment, SinglePayer},
};

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MilitaryUnit {
    base_id: BaseId,
    position: Point,
}

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(_payment: Payment<Self, SinglePayer>, base_id: BaseId, position: Point) -> Self {
        Self { base_id, position }
    }

    pub(crate) fn base_id(&self) -> BaseId {
        self.base_id
    }
}
