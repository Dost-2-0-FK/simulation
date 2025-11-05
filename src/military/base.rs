use std::collections::HashSet;

use crate::{
    money::{Money, ResourceValue},
    placement::Placement,
    politics::Zone,
};

/// A [MilitaryBase] is built on a placement, and associated with a [Zone] and a [Bloc]. The associations are given
/// implicitly via the [Placement], as well as its the position/coordinates.
#[derive(Debug, Clone)]
pub(crate) struct MilitaryBase {
    placement: Placement,
    /// How much credit has been produced since the last full hour?
    production_count: Money,
}

pub(crate) struct MilitaryBaseCost {
    money: Money,
    resources: HashSet<ResourceValue>,
}
