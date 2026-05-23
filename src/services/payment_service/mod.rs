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
    domain::{BlocName, MilitaryBase, MilitaryUnit, Trust},
    handlers::bases::Financing,
};

pub(crate) struct SinglePayer;

pub(crate) struct Financiers {
    financiers: Vec<Financing>,
}

/// Produced by the payment service when a cost was paid.
#[derive(Debug)]
pub(crate) struct Payment<'a, T, P> {
    policy: P,
    cost: &'a Cost<T>,
}

impl<'a, T, P> Payment<'a, T, P> {
    #[expect(dead_code)]
    pub(crate) fn cost(&self) -> &Cost<T> {
        self.cost
    }
}

impl<'a, T> Payment<'a, T, SinglePayer> {
    pub(crate) fn new(cost: &'a Cost<T>) -> Self {
        Self {
            cost,
            policy: SinglePayer,
        }
    }
}

impl<'a, T> Payment<'a, T, Financiers> {
    pub(crate) fn new(cost: &'a Cost<T>, policy: Financiers) -> Self {
        Self { cost, policy }
    }

    pub(crate) fn consume(self) -> Vec<Financing> {
        self.policy.financiers
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

    pub(crate) fn pay_for_military_unit(&self) -> Payment<'_, MilitaryUnit, SinglePayer> {
        self.log_payment(&self.military_unit);
        Payment::<_, SinglePayer>::new(&self.military_unit)
    }

    /// Returns the hourly income for the given bloc.
    ///
    /// # TODO
    /// Make an HTTP request to `CREDIT-EXCHANGE-SERVICE/api/credits/hourly?id=<bloc_id>`.
    pub(crate) async fn hourly_income(&self, _bloc_name: &BlocName) -> f64 {
        0.0
    }

    pub(crate) async fn pay_for_military_base(
        &'_ self,
        financiers: Vec<Financing>,
    ) -> Payment<'_, MilitaryBase, Financiers> {
        log::info!("booking military base payment with {:?}", financiers,);
        self.log_payment(&self.military_base);
        Payment::<_, Financiers>::new(&self.military_base, Financiers { financiers })
    }

    pub(crate) async fn pay_for_trust(&'_ self, financiers: Vec<Financing>) -> Payment<'_, Trust, Financiers> {
        log::info!("booking trust payment with {:?}", financiers,);
        self.log_payment(&self.trust);
        Payment::<_, Financiers>::new(&self.trust, Financiers { financiers })
    }
}
