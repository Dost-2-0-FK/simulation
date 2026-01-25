//! This module contains the interface to the payment service and related data structures.

mod cost;
mod money;
mod resources;

pub(crate) use self::{
    cost::Cost,
    money::Money,
    resources::{ResourceValue, Resources, VecResourceName},
};
use crate::{
    military::{MilitaryBase, MilitaryUnit},
    service::PaymentInfo,
};

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
    pub(crate) military_base: &'a Cost<MilitaryBase>,
}

impl PaymentService<'_> {
    #[inline]
    fn log_payment<T: std::fmt::Debug>(&self, cost: &Cost<T>) {
        log::debug!("issuing payment of {:?}", cost);
    }

    pub(crate) fn pay_for_military_unit(&self) -> Payment<'_, MilitaryUnit> {
        self.log_payment(self.military_unit);
        Payment(self.military_unit)
    }

    pub(crate) async fn pay_for_militray_base(&self, payment_info: &PaymentInfo) -> Payment<'_, MilitaryBase> {
        log::info!(
            "booking military base payment with {:?} and {:?}",
            payment_info.financier_id,
            payment_info.percentage,
        );
        self.log_payment(self.military_base);
        Payment(self.military_base)
    }
}
