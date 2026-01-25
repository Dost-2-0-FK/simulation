//! This module contains the interface to the payment service and related data structures.

mod cost;
mod money;
mod resources;

pub(crate) use self::{
    cost::Cost,
    money::Money,
    resources::{ResourceValue, Resources, VecResourceName},
};
use crate::military::MilitaryUnit;

/// Produced by the payment service when a cost was paid.
#[derive(Debug)]
pub(crate) struct Payment<'a, T>(&'a Cost<T>);

impl<'a, T> Payment<'a, T> {
    pub(crate) fn cost(&self) -> &Cost<T> {
        self.0
    }
}

/// Constructed when loading the config
// TODO quite likely that in the end this works entirely different; we just need some struct to hold the costs
// to enable constructing the `CostPaid` to instantiate a `MilitaryUnit`.
pub(crate) struct PaymentService<'a> {
    pub(crate) military_unit: &'a Cost<MilitaryUnit>,
}

impl PaymentService<'_> {
    pub(crate) fn pay_for_military_unit(&self) -> Payment<'_, MilitaryUnit> {
        log::debug!("issuing payment of {:?}", self.military_unit);
        Payment(self.military_unit)
    }
}
