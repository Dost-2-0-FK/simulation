use std::{any::type_name, marker::PhantomData};

use anyhow::{Result, bail};
use derive_more::Display;
use serde::Deserialize;

use crate::{
    domain::{BlocKey, MilitaryUnit},
    handlers::bases::Financing,
    services::credit_exchange_service::{CreditExchangeService, Money, ResourceValue, Resources, Share},
};

#[derive(Debug, Deserialize, Display)]
#[display("{}: {} ({})", type_name::<T>(), money, resources)]
#[serde(deny_unknown_fields)]
pub(crate) struct Cost<T> {
    #[serde(skip)]
    _id: PhantomData<T>,
    money: Money,
    resources: Resources,
}

impl<T> Cost<T> {
    pub(crate) fn money(&self) -> Money {
        self.money
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = ResourceValue<'_>> {
        self.resources.into_iter()
    }

    pub(crate) fn resources_owned(&self) -> Resources {
        self.resources.clone()
    }
}

/// Produced by the credit exchange service when a cost was paid.
#[derive(Debug)]
pub(crate) struct Payment<'a, T, P> {
    policy: P,
    cost: &'a Cost<T>,
}

#[derive(Debug, Clone)]
pub(super) struct PayerShare {
    pub(crate) payer_id: String,
    pub(crate) share: Share,
}

pub(super) trait Payers {
    fn payers(&self) -> impl Iterator<Item = PayerShare> + Send;
}

/// The `SinglePayer` policyl
pub(crate) struct SinglePayer(pub(super) String);

impl Payers for SinglePayer {
    fn payers(&self) -> impl Iterator<Item = PayerShare> + Send {
        std::iter::once(PayerShare {
            payer_id: self.0.clone(),
            share: 1.0.into(),
        })
    }
}

/// The `Financiers` policy
#[derive(Clone)]
pub(crate) struct Financiers {
    primary_payer_id: String,
    secondary_payers: Vec<PayerShare>,
}

impl Payers for Financiers {
    fn payers(&self) -> impl Iterator<Item = PayerShare> + Send {
        let mut payers = Vec::with_capacity(self.secondary_payers.len() + 1);
        let financier_share = self
            .secondary_payers
            .iter()
            .map(|financing| financing.share.value())
            .sum::<f32>();
        let primary_share = 1.0 - financier_share;
        if primary_share > f32::EPSILON {
            payers.push(PayerShare {
                payer_id: self.primary_payer_id.clone(),
                share: Share::from(primary_share),
            });
        }
        payers.extend(self.secondary_payers.iter().map(|financing| PayerShare {
            payer_id: financing.payer_id.clone(),
            share: financing.share,
        }));

        payers.into_iter()
    }
}

impl<'a, T, P> Payment<'a, T, P> {
    pub(crate) fn cost(&self) -> &Cost<T> {
        self.cost
    }

    pub(crate) fn policy(&self) -> &P {
        &self.policy
    }
}

impl<'a, T> Payment<'a, T, Financiers> {
    pub(crate) fn secondary_payers(self) -> Vec<Financing> {
        self.policy()
            .secondary_payers
            .iter()
            .map(|payer| Financing {
                financier: payer.payer_id.clone().into(),
                share: payer.share,
            })
            .collect()
    }
}

impl CreditExchangeService {
    pub(crate) async fn pay_for_military_unit(
        &self,
        payer: &BlocKey,
    ) -> Result<Payment<'_, MilitaryUnit, SinglePayer>> {
        self.log_payment(&self.military_unit);
        let policy = SinglePayer(payer.to_string());
        self.book_cost(policy, &self.military_unit).await
    }

    pub(super) async fn book_cost<'a, T, P>(&self, policy: P, cost: &'a Cost<T>) -> Result<Payment<'a, T, P>>
    where
        T: std::fmt::Debug,
        P: Payers + Send,
    {
        let payers = policy.payers();
        for payer in payers {
            self.book_credit(
                payer.payer_id.as_str(),
                &self.bank_user_id,
                "money",
                payer.share * cost.money(),
            )
            .await?;
            for resource in cost.resources() {
                self.book_resource(payer.payer_id.as_str(), &self.bank_user_id, &resource, payer.share)
                    .await?;
            }
        }
        Ok(Payment { policy, cost })
    }
}

impl Financiers {
    pub(super) fn new(primary_payer_id: String, financiers: Vec<Financing>) -> Result<Self> {
        let financier_share = financiers.iter().map(|financing| financing.share.value()).sum::<f32>();
        if financier_share > 1.0 {
            bail!("financier shares sum to {financier_share}, expected at most 1");
        }
        Ok(Self {
            primary_payer_id,
            secondary_payers: financiers
                .iter()
                .map(|financing| PayerShare {
                    payer_id: financing.financier.as_str().to_string(),
                    share: financing.share,
                })
                .collect(),
        })
    }

    pub(super) fn primary_payer_id(&self) -> &str {
        &self.primary_payer_id
    }
}
