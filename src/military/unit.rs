use serde::Serialize;

use crate::{
    geometry::{Point, Positioned},
    military::base::BaseId,
    payment_service::Payment,
};

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MilitaryUnit {
    base_id: BaseId,
    position: Point,
}

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(_payment: Payment<Self>, base_id: BaseId, position: Point) -> Self {
        Self { base_id, position }
    }
}
