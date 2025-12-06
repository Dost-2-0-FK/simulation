use std::collections::HashSet;

use crate::{
    geometry::{Point, Positioned},
    military::base::MilitaryBase,
    money::{Money, ResourceValue},
};

/// Associated with a [MilitaryBase] and a [Bloc]. The [Bloc] association is implicit.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnit {
    base: MilitaryBase,
    position: Point,
}

crate::impl_positioned!(MilitaryUnit => position);

#[derive(Debug, Clone)]
pub(crate) struct MilitaryUnitCost {
    money: Money,
    resource: HashSet<ResourceValue>,
}
