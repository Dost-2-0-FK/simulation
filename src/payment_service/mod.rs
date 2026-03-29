//! This module contains the interface to the payment service and related data structures.

mod cost;
mod money;
mod resources;

use url::Url;

pub(crate) use self::{
    cost::Cost,
    money::Money,
    resources::{ResourceValue, Resources, VecResourceName},
};
use crate::{
    military::{MilitaryBase, MilitaryUnit},
    service::bases::PaymentInfo,
    trust::Trust,
};

/// Produced by the payment service when a cost was paid.
#[derive(Debug)]
pub(crate) struct Payment<'a, T>(&'a Cost<T>);

impl<'a, T> Payment<'a, T> {
    #[expect(dead_code)]
    pub(crate) fn cost(&self) -> &Cost<T> {
        self.0
    }
}

/// Constructed when loading the config
// TODO quite likely that in the end this works entirely different; we just need some struct to hold the costs
// to enable constructing the `CostPaid` to instantiate a `MilitaryUnit`.
#[expect(dead_code)]
pub(crate) struct PaymentService {
    url: Url,
    pub(crate) military_unit: Cost<MilitaryUnit>,
    pub(crate) trust: Cost<Trust>,
    pub(crate) military_base: Cost<MilitaryBase>,
}

impl PaymentService {
    pub(crate) fn new(
        url: Url,
        military_unit_cost: Cost<MilitaryUnit>,
        military_base_cost: Cost<MilitaryBase>,
        trust_cost: Cost<Trust>,
    ) -> Self {
        Self {
            url,
            military_unit: military_unit_cost,
            trust: trust_cost,
            military_base: military_base_cost,
        }
    }

    #[inline]
    fn log_payment<T: std::fmt::Debug>(&self, cost: &Cost<T>) {
        log::debug!("issuing payment of {}", cost);
    }

    pub(crate) fn pay_for_military_unit(&self) -> Payment<'_, MilitaryUnit> {
        self.log_payment(&self.military_unit);
        Payment(&self.military_unit)
    }

    pub(crate) async fn pay_for_militray_base(&'_ self, payment_info: &PaymentInfo) -> Payment<'_, MilitaryBase> {
        log::info!(
            "booking military base payment with {:?} and {:?}",
            payment_info.financier_id,
            payment_info.percentage,
        );
        self.log_payment(&self.military_base);
        Payment(&self.military_base)
    }
}
