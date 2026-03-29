use crate::{
    geometry::{Point, Positioned},
    payment_service::{Money, ResourceValue},
    placement::Placement,
};

/// A [Trust] is built on a placement, and associated with a [Zone]. The association is given
/// implicitly via the [Placement]. Also, the position/coordinates are given via the [Placement].
#[derive(Debug, Clone)]
#[expect(dead_code)]
pub(crate) struct Trust {
    placement: Placement,
    income: TrustIncome,
}

crate::impl_positioned!(Trust => placement);

/// Instantiated with fixed base values, mutated during simulation.
#[derive(Debug, Clone)]
#[expect(dead_code)]
struct TrustIncome {
    /// Based on a fixed value, negatively influenced by close enemy military units and updated hourly.
    cash: Money,
    /// Based on a fixed value, negatively influenced by close enemy military units and updated hourly.
    /// Also influenced by the spent resources of that unit and the current production of that resource unit
    /// in the _other_ bloc.
    resource: ResourceValue<'static>,
}
