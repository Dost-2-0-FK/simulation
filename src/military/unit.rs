use std::collections::HashSet;

use serde::Serialize;

use crate::{
    geometry::{Point, Positioned},
    military::base::{BaseId, MilitaryBase},
    money::{Money, Payment, ResourceValue},
};

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct MilitaryUnit {
    base_id: BaseId,
    position: Point,
}

crate::impl_positioned!(MilitaryUnit => position);

impl MilitaryUnit {
    pub(crate) fn new(_payment: Payment<MilitaryUnitCost>, base_id: BaseId, position: Point) -> Self {
        Self { base_id, position }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnitCost {
    money: Money,
    resource: HashSet<ResourceValue>,
}

impl Default for MilitaryUnitCost {
    fn default() -> Self {
        Self {
            money: Default::default(),
            resource: Default::default(),
        }
    }
}
