//! This module contains the interface to the credit exchange service and related data structures.

mod cost;
mod money;
mod resources;
mod share;

use anyhow::{Context, Result, anyhow};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use url::Url;

pub(crate) use self::{
    cost::{Cost, Financiers, Payment, SinglePayer},
    money::Money,
    resources::{ResourceName, ResourceValue, Resources, ResourcesFactors, VecResourceName},
    share::Share,
};
use crate::{
    domain::{BaseId, BlocName, Loot, LootFactors, MilitaryBase, MilitaryUnit, Trust, TrustId, ZoneName},
    handlers::bases::Financing,
    services::credit_exchange_service::cost::{PayerShare, Payers},
};

#[derive(Debug, Clone, Copy)]
enum UserType {
    Unit,
}

impl Serialize for UserType {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Unit => serializer.serialize_str("unit"),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateUserRequest {
    id: String,
    user_type: UserType,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreditBooking {
    credit_type: String,
    receiver: String,
    value: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CreateSubscriptionRequest {
    receiver: String,
    value: f32,
    subscription_type: &'static str,
    priority: u32,
    credit_type: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PatchUserRequest {
    credit_type: String,
    last_day_average: f32,
}

#[derive(Debug, Deserialize)]
struct ListCreditsResponse {
    credits: Vec<CreditBalanceResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreditBalanceResponse {
    credit_type: String,
    hourly: f32,
}

/// Constructed when loading the config. It owns the local simulation costs and provides the
/// small subset of credit-exchanger API calls used by this simulation.
pub(crate) struct CreditExchangeService {
    client: reqwest::Client,
    url: Url,
    bank_user_id: String,
    pub(crate) military_unit: Cost<MilitaryUnit>,
    pub(crate) trust: Cost<Trust>,
    pub(crate) military_base: Cost<MilitaryBase>,
    loot_factors: LootFactors,
}

impl CreditExchangeService {
    pub(crate) fn new(
        url: Url,
        bank_user_id: String,
        military_unit_cost: Cost<MilitaryUnit>,
        military_base_cost: Cost<MilitaryBase>,
        trust_cost: Cost<Trust>,
        loot_factors: LootFactors,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
            bank_user_id,
            military_unit: military_unit_cost,
            trust: trust_cost,
            military_base: military_base_cost,
            loot_factors,
        }
    }

    pub(crate) fn loot_factors(&self) -> &LootFactors {
        &self.loot_factors
    }

    #[inline]
    fn log_payment<T: std::fmt::Debug>(&self, cost: &Cost<T>) {
        log::debug!("issuing payment of {}", cost);
    }

    /// Returns the hourly income for the given bloc.
    pub(crate) async fn hourly_income(&self, bloc_name: &BlocName) -> (Money, Resources) {
        match self.credit_hourly_income(&bloc_name.to_string()).await {
            Ok(income) => income,
            Err(err) => {
                log::error!("failed to fetch hourly income for bloc {bloc_name}: {err}");
                (Money::default(), Resources::default())
            }
        }
    }

    pub(crate) async fn pay_for_military_base(
        &self,
        primary_payer_id: &BlocName,
        financiers: Vec<Financing>,
    ) -> Result<Payment<'_, MilitaryBase, Financiers>> {
        self.log_payment(&self.military_base);
        let policy = Financiers::new(primary_payer_id.to_string(), financiers)?;
        self.log_payment(&self.military_base);
        self.book_cost(policy, &self.military_base).await
    }

    pub(crate) async fn register_military_base(&self, base: &MilitaryBase, policy: Financiers) -> Result<()> {
        let producer = Self::base_credit_user_id(base.id());
        self.ensure_unit_user(&producer).await?;
        self.create_cost_subscriptions(policy.payers(), &producer, &self.military_base)
            .await?;
        Ok(())
    }

    pub(crate) async fn pay_for_trust(
        &self,
        primary_payer_id: &ZoneName,
        financiers: Vec<Financing>,
    ) -> Result<Payment<'_, Trust, Financiers>> {
        self.log_payment(&self.trust);
        let policy = Financiers::new(primary_payer_id.to_string(), financiers)?;
        self.book_cost(policy, &self.trust).await
    }

    pub(crate) async fn register_trust(&self, trust: &Trust, policy: &Financiers) -> Result<()> {
        let producer = Self::trust_credit_user_id(trust.id());
        self.ensure_unit_user(&producer).await?;
        self.create_cost_subscriptions(policy.payers(), &producer, &self.trust)
            .await?;
        Ok(())
    }

    pub(crate) async fn set_base_production(&self, base_id: BaseId, value: &Loot) -> Result<()> {
        self.set_credit_production(&Self::base_credit_user_id(base_id), value.money())
            .await?;
        for resource in value.resources() {
            self.set_resource_production(&Self::base_credit_user_id(base_id), resource.name(), resource.value())
                .await?;
        }
        Ok(())
    }

    #[expect(dead_code)]
    pub(crate) async fn set_trust_credit_production(&self, trust_id: TrustId, value: Money) -> Result<()> {
        self.set_credit_production(&Self::trust_credit_user_id(trust_id), value)
            .await
    }

    #[expect(dead_code)]
    pub(crate) async fn set_trust_resource_production(
        &self,
        trust_id: TrustId,
        resource: &ResourceName,
        value: f32,
    ) -> Result<()> {
        self.set_resource_production(&Self::trust_credit_user_id(trust_id), resource, value)
            .await
    }

    pub(crate) async fn credit_hourly_income(&self, user_id: &str) -> Result<(Money, Resources)> {
        let url = self.endpoint(&format!("api/users/{user_id}/credits"))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("requesting hourly credit income")?;
        let response = Self::error_for_status(response, "requesting hourly credit income").await?;
        let response = response
            .json::<ListCreditsResponse>()
            .await
            .context("decoding hourly credit income")?;

        let mut money = Money::default();
        let mut resources = Resources::default();
        for credit in response.credits {
            if credit.credit_type == "money" {
                money = Money::from(credit.hourly);
            } else {
                resources.insert(ResourceName::new(credit.credit_type), credit.hourly);
            }
        }

        Ok((money, resources))
    }

    async fn ensure_unit_user(&self, id: &str) -> Result<()> {
        let url = self.endpoint("api/users")?;
        let response = self
            .client
            .post(url)
            .json(&CreateUserRequest {
                id: id.to_string(),
                user_type: UserType::Unit,
            })
            .send()
            .await
            .with_context(|| format!("creating credit-exchanger unit user {id}"))?;

        if response.status() == StatusCode::CONFLICT {
            return Ok(());
        }

        Self::error_for_status(response, &format!("creating credit-exchanger unit user {id}")).await?;
        Ok(())
    }

    async fn book_credit(&self, payer: &str, receiver: &str, credit_type: &str, value: Money) -> Result<()> {
        self.book_value(payer, receiver, credit_type, value.value()).await
    }

    async fn book_resource(
        &self,
        payer: &str,
        receiver: &str,
        resource: &ResourceValue<'_>,
        share: Share,
    ) -> Result<()> {
        self.book_value(
            payer,
            receiver,
            resource.name().as_str(),
            resource.value() * share.value(),
        )
        .await
    }

    async fn book_value(&self, payer: &str, receiver: &str, credit_type: &str, value: f32) -> Result<()> {
        if value <= 0.0 {
            return Ok(());
        }

        let url = self.endpoint(&format!("api/users/{payer}/bookings"))?;
        let response = self
            .client
            .post(url)
            .json(&CreditBooking {
                credit_type: credit_type.to_string(),
                receiver: receiver.to_string(),
                value,
            })
            .send()
            .await
            .with_context(|| format!("booking {value} {credit_type} from {payer} to {receiver}"))?;
        Self::error_for_status(
            response,
            &format!("booking {value} {credit_type} from {payer} to {receiver}"),
        )
        .await?;
        Ok(())
    }

    async fn create_cost_subscriptions<T: std::fmt::Debug>(
        &self,
        payers: impl IntoIterator<Item = PayerShare>,
        producer: &str,
        cost: &Cost<T>,
    ) -> Result<()> {
        for payer in payers {
            self.create_subscription(producer, payer.payer_id.as_str(), "money", payer.share)
                .await?;
            for resource in cost.resources() {
                self.create_subscription(producer, payer.payer_id.as_str(), resource.name().as_str(), payer.share)
                    .await?;
            }
        }
        Ok(())
    }

    async fn create_subscription(&self, producer: &str, receiver: &str, credit_type: &str, share: Share) -> Result<()> {
        let value = share.value() * 100.0;
        if value <= 0.0 {
            return Ok(());
        }

        let url = self.endpoint(&format!("api/users/{producer}/subscriptions"))?;
        let response = self
            .client
            .post(url)
            .json(&CreateSubscriptionRequest {
                receiver: receiver.to_string(),
                value,
                subscription_type: "contract",
                priority: 0,
                credit_type: credit_type.to_string(),
            })
            .send()
            .await
            .with_context(|| format!("creating {credit_type} subscription from {producer} to {receiver}"))?;
        Self::error_for_status(
            response,
            &format!("creating {credit_type} subscription from {producer} to {receiver}"),
        )
        .await?;
        Ok(())
    }

    async fn set_credit_production(&self, user_id: &str, value: Money) -> Result<()> {
        self.set_production(user_id, "money", value.value()).await
    }

    async fn set_resource_production(&self, user_id: &str, resource: &ResourceName, value: f32) -> Result<()> {
        self.set_production(user_id, resource.as_str(), value).await
    }

    async fn set_production(&self, user_id: &str, credit_type: &str, value: f32) -> Result<()> {
        let url = self.endpoint(&format!("api/users/{user_id}"))?;
        let response = self
            .client
            .patch(url)
            .json(&PatchUserRequest {
                credit_type: credit_type.to_string(),
                last_day_average: value,
            })
            .send()
            .await
            .with_context(|| format!("setting {credit_type} production for {user_id}"))?;
        Self::error_for_status(response, &format!("setting {credit_type} production for {user_id}")).await?;
        Ok(())
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.url
            .join(path)
            .with_context(|| format!("building credit-exchanger endpoint for {path}"))
    }

    async fn error_for_status(response: reqwest::Response, action: &str) -> Result<reqwest::Response> {
        if response.status().is_success() {
            return Ok(response);
        }

        let status = response.status();
        let body = response.text().await.unwrap_or_else(|err| err.to_string());
        Err(anyhow!("{action} failed with {status}: {body}"))
    }

    fn base_credit_user_id(id: BaseId) -> String {
        format!("base-{}", id.0)
    }

    fn trust_credit_user_id(id: TrustId) -> String {
        format!("trust-{}", id.0)
    }
}
