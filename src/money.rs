use std::sync::Arc;

use crate::military::MilitaryUnitCost;

#[derive(Debug, Clone)]
pub(crate) struct Money(f32);

impl Default for Money {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResouceName(String);

#[derive(Debug, Clone)]
pub(crate) struct ResourceValue(ResouceName, f32);

/// Produced by the payment service when a cost was paid.
#[derive(Debug)]
pub(crate) struct Payment<T>(Arc<T>);

impl<T> Payment<T> {
    pub(crate) fn cost(&self) -> &T {
        &self.0
    }
}

/// Constructed when loading the config
// TODO quite likely that in the end this works entirely different; we just need some struct to hold the costs
// to enable constructing the `CostPaid` to instantiate a `MilitaryUnit`.
pub(crate) struct Costs {
    pub(crate) military_unit: Arc<MilitaryUnitCost>,
}

impl Costs {
    pub(crate) fn pay_for_military_unit(&self) -> Payment<MilitaryUnitCost> {
        Payment(self.military_unit.clone())
    }
}
