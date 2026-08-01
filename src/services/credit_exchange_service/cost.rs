use std::{any::type_name, collections::HashMap, marker::PhantomData};

use anyhow::{Result, bail};
use derive_more::Display;
use serde::Deserialize;

use crate::{
    domain::{BlocKey, MilitaryUnit, Trust},
    handlers::bases::Financing,
    services::credit_exchange_service::{CreditExchangeService, Money, ResourceValue, Resources, Share},
};

use super::{CreditUserId, ResourceName};

#[derive(Debug, Clone, Deserialize, Display)]
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

#[derive(Debug, Clone, Deserialize)]
#[serde(transparent)]
pub(crate) struct TrustCosts(HashMap<ResourceName, Cost<Trust>>);

impl TrustCosts {
    pub(crate) fn get(&self, resource: &ResourceName) -> Option<&Cost<Trust>> {
        self.0.get(resource)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&ResourceName, &Cost<Trust>)> {
        self.0.iter()
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
    pub(crate) payer_id: CreditUserId,
    pub(crate) share: Share,
}

pub(super) trait Payers {
    fn payers(&self) -> impl Iterator<Item = PayerShare> + Send;

    fn resource_payer(&self) -> CreditUserId;
}

/// The `SinglePayer` policyl
pub(crate) struct SinglePayer(pub(super) CreditUserId);

impl Payers for SinglePayer {
    fn payers(&self) -> impl Iterator<Item = PayerShare> + Send {
        std::iter::once(PayerShare {
            payer_id: self.0.clone(),
            share: 1.0.into(),
        })
    }

    fn resource_payer(&self) -> CreditUserId {
        self.0.clone()
    }
}

/// The `Financiers` policy
#[derive(Debug, Clone)]
pub(crate) struct Financiers {
    primary_payer_id: CreditUserId,
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

    fn resource_payer(&self) -> CreditUserId {
        self.primary_payer_id.clone()
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
                financier: payer.payer_id.as_str().to_string().into(),
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
        let policy = SinglePayer(CreditUserId::from(payer));
        self.book_cost(policy, &self.military_unit).await
    }

    pub(super) async fn book_cost<'a, T, P>(&self, policy: P, cost: &'a Cost<T>) -> Result<Payment<'a, T, P>>
    where
        T: std::fmt::Debug,
        P: Payers + Send,
    {
        let obligations = payment_obligations(&policy, cost);
        self.ensure_balances_cover(&obligations).await?;

        for obligation in obligations {
            self.book_credit(&obligation.payer_id, &self.bank_user_id, obligation.money)
                .await?;
            for resource in &obligation.resources {
                self.book_resource(&obligation.payer_id, &self.bank_user_id, &resource)
                    .await?;
            }
        }
        Ok(Payment { policy, cost })
    }
}

#[derive(Debug)]
pub(super) struct PaymentObligation {
    pub(super) payer_id: CreditUserId,
    pub(super) money: Money,
    pub(super) resources: Resources,
}

fn payment_obligations<T, P: Payers>(policy: &P, cost: &Cost<T>) -> Vec<PaymentObligation> {
    let mut obligations = Vec::<PaymentObligation>::new();
    for payer in policy.payers() {
        let money = payer.share * cost.money();
        if let Some(obligation) = obligations.iter_mut().find(|entry| entry.payer_id == payer.payer_id) {
            obligation.money += money;
        } else {
            obligations.push(PaymentObligation {
                payer_id: payer.payer_id,
                money,
                resources: Resources::default(),
            });
        }
    }

    let resource_payer = policy.resource_payer();
    if let Some(obligation) = obligations.iter_mut().find(|entry| entry.payer_id == resource_payer) {
        obligation.resources += &cost.resources_owned();
    } else {
        obligations.push(PaymentObligation {
            payer_id: resource_payer,
            money: Money::default(),
            resources: cost.resources_owned(),
        });
    }
    obligations
}

impl Financiers {
    pub(super) fn new(primary_payer_id: String, financiers: Vec<Financing>) -> Result<Self> {
        let financier_share = financiers.iter().map(|financing| financing.share.value()).sum::<f32>();
        if financier_share > 1.0 {
            bail!("financier shares sum to {financier_share}, expected at most 1");
        }
        Ok(Self {
            primary_payer_id: primary_payer_id.into(),
            secondary_payers: financiers
                .iter()
                .map(|financing| PayerShare {
                    payer_id: CreditUserId::from(&financing.financier),
                    share: financing.share,
                })
                .collect(),
        })
    }

    pub(super) fn primary_payer_id(&self) -> &CreditUserId {
        &self.primary_payer_id
    }
}
