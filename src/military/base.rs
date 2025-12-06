use std::collections::HashSet;

use serde::Serialize;

use crate::{
    geometry::{Point, Positioned},
    money::{Money, ResourceValue},
    placement::Placement,
    politics::Zone,
};

#[derive(Debug, Clone, Copy, Serialize)]
// TODO make this inner field private (just for now it's not to enable a shortcut to create a unit)
pub(crate) struct BaseId(pub(crate) u64);

/// A [MilitaryBase] is built on a placement, and associated with a [Zone] and a [Bloc]. The associations are given
/// implicitly via the [Placement], as well as its the position/coordinates.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryBase {
    id: BaseId,
    placement: Placement,
    /// How much credit has been produced since the last full hour?
    production_count: Money,
}

crate::impl_positioned!(MilitaryBase => placement);

pub(crate) struct MilitaryBaseCost {
    money: Money,
    resources: HashSet<ResourceValue>,
}
