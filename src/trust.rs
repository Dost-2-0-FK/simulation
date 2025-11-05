use std::collections::HashSet;

use crate::{
    money::{Money, ResourceValue},
    placement::Placement,
};

/// A [Trust] is built on a placement, and associated with a [Zone]. The association is given
/// implicitly via the [Placement]. Also, the position/coordinates are given via the [Placement].
#[derive(Debug, Clone)]
struct Trust {
    placement: Placement,
    income: TrustIncome,
}

#[derive(Debug, Clone)]
struct TrustCost {
    money: Money,
    resources: HashSet<ResourceValue>,
}

/// Instantiated with fixed base values, mutated during simulation.
#[derive(Debug, Clone)]
struct TrustIncome {
    /// Based on a fixed value, negatively influenced by close enemy military units and updated hourly.
    cash: Money,
    /// Based on a fixed value, negatively influenced by close enemy military units and updated hourly.
    /// Also influenced by the spent resources of that unit and the current production of that resource unit
    /// in the _other_ bloc.
    resource: ResourceValue,
}
