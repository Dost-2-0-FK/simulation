//! This module contains the interface to the credit exchange service and related data structures.

mod cost;
mod money;
mod resources;
mod share;

use std::collections::HashMap;

use anyhow::{Context, Result};
use derive_more::derive::{Display, Error};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use url::Url;

pub(crate) use self::{
    cost::{Cost, Financiers, Payment, SinglePayer},
    money::Money,
    resources::{MoneyPerResource, ResourceName, ResourceValue, Resources, ResourcesFactors, VecResourceName},
    share::Share,
};
use crate::{
    domain::{
        BaseId, BlocKey, BlocName, CharacterKey, CharacterName, Loot, LootFactors, MilitaryBase, MilitaryUnit,
        NameMappings, ProductionUnit, ProductionUnitKey, Trust, TrustId, ZoneKey, ZoneName,
    },
    handlers::bases::Financing,
    services::credit_exchange_service::cost::Payers,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct CreditUserId(String);

impl CreditUserId {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&BlocKey> for CreditUserId {
    fn from(value: &BlocKey) -> Self {
        Self(value.as_str().to_string())
    }
}

impl From<&ZoneKey> for CreditUserId {
    fn from(value: &ZoneKey) -> Self {
        Self(value.as_str().to_string())
    }
}

impl From<&CharacterKey> for CreditUserId {
    fn from(value: &CharacterKey) -> Self {
        Self(value.as_str().to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub(crate) struct SubscriptionId(String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
pub(crate) struct CreditType(String);

impl CreditType {
    fn is_money(&self) -> bool {
        self.0 == "money"
    }

    fn into_resource_name(self) -> ResourceName {
        ResourceName::new(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum SubscriptionType {
    Sr,
    Contract,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Subscription {
    id: SubscriptionId,
    sender: CreditUserName,
    receiver: CreditUserName,
    value: f32,
    subscription_type: SubscriptionType,
    priority: u32,
    credit_type: CreditType,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub(crate) enum CreditUserName {
    Character { name: CharacterName },
    Bloc { name: BlocName },
    Zone { name: ZoneName },
    Other { id: CreditUserId },
}

impl CreditUserName {
    fn resolve(id: CreditUserId, name_mappings: &NameMappings) -> Result<Self> {
        let character = name_mappings
            .character_name(&CharacterKey::from(id.as_str().to_string()))
            .map(|name| Self::Character { name: name.clone() });
        let bloc = name_mappings
            .bloc_name(&BlocKey::from(id.as_str().to_string()))
            .map(|name| Self::Bloc { name: name.clone() });
        let zone = name_mappings
            .zone_name(&ZoneKey::from(id.as_str().to_string()))
            .map(|name| Self::Zone { name: name.clone() });
        let mut matches = [character, bloc, zone].into_iter().flatten();

        let Some(name) = matches.next() else {
            return Ok(Self::Other { id });
        };
        if matches.next().is_some() {
            anyhow::bail!("credit user ID {id} matches multiple configured names");
        }
        Ok(name)
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CreditAccount {
    money: MoneyBalance,
    resources: HashMap<ResourceName, ResourceBalance>,
    subscriptions: AccountSubscriptions,
}

#[derive(Debug, Clone, Default, Serialize, utoipa::ToSchema)]
pub(crate) struct MoneyBalance {
    balance: Money,
    hourly: Money,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct ResourceBalance {
    balance: f32,
    hourly: f32,
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub(crate) struct AccountSubscriptions {
    outgoing: Vec<Subscription>,
    incoming: Vec<Subscription>,
}

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

#[derive(Debug, PartialEq)]
struct SubscriptionSpec {
    receiver: String,
    credit_type: String,
    share: Share,
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
    credit_type: CreditType,
    balance: f32,
    hourly: f32,
}

#[derive(Debug, Deserialize)]
struct CreditUserResponse {
    id: String,
    #[serde(default)]
    resources: HashMap<ResourceName, CreditTotalResponse>,
}

#[derive(Debug, Deserialize)]
struct CreditTotalResponse {
    total: f32,
}

#[derive(Debug, Deserialize)]
struct ListSubscriptionsResponse {
    subscriptions: Vec<SubscriptionResponse>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionResponse {
    id: SubscriptionId,
    sender: CreditUserId,
    receiver: CreditUserId,
    value: f32,
    subscription_type: SubscriptionType,
    priority: u32,
    credit_type: CreditType,
}

/// Constructed when loading the config. It owns the local simulation costs and provides the
/// small subset of credit-exchanger API calls used by this simulation.
#[derive(Clone)]
pub(crate) struct CreditExchangeService {
    client: reqwest::Client,
    url: Url,
    bank_user_id: String,
    resources: VecResourceName,
    pub(crate) military_unit: Cost<MilitaryUnit>,
    pub(crate) trust: Cost<Trust>,
    pub(crate) military_base: Cost<MilitaryBase>,
    loot_factors: LootFactors,
}

/// A non-successful HTTP response returned by the credit-exchange service.
///
/// Keeping this error typed lets API handlers forward expected upstream failures (such as an
/// insufficient-credit response) without treating transport and decoding failures as user errors.
#[derive(Debug, Display, Error)]
#[display("credit-exchange service returned {status}: {body}")]
pub(crate) struct CreditExchangeResponseError {
    status: StatusCode,
    body: String,
}

impl CreditExchangeResponseError {
    pub(crate) fn new(status: StatusCode, body: String) -> Self {
        Self { status, body }
    }

    pub(crate) fn status(&self) -> StatusCode {
        self.status
    }

    pub(crate) fn body(&self) -> &str {
        &self.body
    }

    pub(crate) fn is_insufficient_credit(&self) -> bool {
        self.status == StatusCode::BAD_REQUEST && self.body.trim() == "Insufficient credit for booking"
    }
}

impl CreditExchangeService {
    pub(crate) fn new(
        url: Url,
        bank_user_id: String,
        military_unit_cost: Cost<MilitaryUnit>,
        military_base_cost: Cost<MilitaryBase>,
        trust_cost: Cost<Trust>,
        resources: VecResourceName,
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
            resources,
        }
    }

    pub(crate) fn loot_factors(&self) -> &LootFactors {
        &self.loot_factors
    }

    pub(crate) fn resources(&self) -> &[ResourceName] {
        &self.resources
    }

    #[inline]
    fn log_payment<T: std::fmt::Debug>(&self, cost: &Cost<T>) {
        log::debug!("issuing payment of {}", cost);
    }

    /// Returns the hourly income for the given bloc.
    pub(crate) async fn hourly_income(&self, bloc_key: &BlocKey) -> (Money, Resources) {
        match self.credit_hourly_income(bloc_key.as_str()).await {
            Ok(income) => income,
            Err(err) => {
                log::error!("failed to fetch hourly income for bloc {bloc_key}: {err}");
                (Money::default(), Resources::default())
            }
        }
    }

    pub(crate) async fn pay_for_military_base(
        &self,
        primary_payer_id: &BlocKey,
        financiers: Vec<Financing>,
    ) -> Result<Payment<'_, MilitaryBase, Financiers>> {
        self.log_payment(&self.military_base);
        let policy = Financiers::new(primary_payer_id.to_string(), financiers)?;
        self.log_payment(&self.military_base);
        self.book_cost(policy, &self.military_base).await
    }

    pub(crate) async fn register_military_base(&self, base: &MilitaryBase, policy: Financiers) -> Result<()> {
        let producer = Self::base_credit_user_id(base.id());
        self.register_producer(&producer, &policy).await?;
        Ok(())
    }

    /// Register a configured base whose financing was completed before simulation startup.
    pub(crate) async fn register_prepaid_military_base(&self, base: &MilitaryBase) -> Result<()> {
        let policy = Financiers::new(base.bloc_key().to_string(), base.financiers().to_vec())?;
        self.register_military_base(base, policy).await
    }

    pub(crate) async fn pay_for_trust(
        &self,
        primary_payer_id: &ZoneKey,
        financiers: Vec<Financing>,
    ) -> Result<Payment<'_, Trust, Financiers>> {
        self.log_payment(&self.trust);
        let policy = Financiers::new(primary_payer_id.to_string(), financiers)?;
        self.book_cost(policy, &self.trust).await
    }

    pub(crate) async fn register_trust(&self, trust: &Trust, policy: &Financiers) -> Result<()> {
        let producer = Self::trust_credit_user_id(trust.id());
        self.register_producer(&producer, policy).await?;
        Ok(())
    }

    /// Register a configured trust whose financing was completed before simulation startup.
    pub(crate) async fn register_prepaid_trust(&self, trust: &Trust) -> Result<()> {
        let policy = Financiers::new(trust.placement().zone().key().to_string(), trust.financing().to_vec())?;
        self.register_trust(trust, &policy).await
    }

    /// Register a configured production unit as the same producer type used by trusts.
    pub(crate) async fn register_prepaid_production_unit(&self, unit: &ProductionUnit) -> Result<()> {
        let policy = Financiers::new(unit.zone().key().to_string(), Vec::new())?;
        let producer = Self::production_unit_credit_user_id(unit.key());
        self.register_producer(&producer, &policy).await
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

    pub(crate) async fn set_trust_production(&self, trust: &Trust, income: Money, producing: &Resources) -> Result<()> {
        let producer = Self::trust_credit_user_id(trust.id());
        self.set_producer_production(&producer, income, producing).await
    }

    pub(crate) async fn set_production_unit_production(
        &self,
        unit: &ProductionUnit,
        income: Money,
        producing: &Resources,
    ) -> Result<()> {
        let producer = Self::production_unit_credit_user_id(unit.key());
        self.set_producer_production(&producer, income, producing).await
    }

    async fn set_producer_production(
        &self,
        producer: &CreditUserId,
        income: Money,
        producing: &Resources,
    ) -> Result<()> {
        self.set_credit_production(producer, income).await?;
        for resource in producing {
            self.set_resource_production(producer, resource.name(), resource.value())
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn delete_base_subscriptions(&self, base_id: BaseId) -> Result<()> {
        self.delete_user_subscriptions(&Self::base_credit_user_id(base_id))
            .await
    }

    pub(crate) async fn delete_trust_subscriptions(&self, trust_id: TrustId) -> Result<()> {
        self.delete_user_subscriptions(&Self::trust_credit_user_id(trust_id))
            .await
    }

    pub(crate) async fn resource_totals_excluding_bank(&self) -> Result<Resources> {
        let url = self.endpoint("api/users")?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .context("requesting credit-exchanger users for resource totals")?;
        let response =
            Self::error_for_status(response, "requesting credit-exchanger users for resource totals").await?;
        let users = response
            .json::<Vec<CreditUserResponse>>()
            .await
            .context("decoding credit-exchanger users for resource totals")?;

        Ok(sum_resource_totals(users, &self.bank_user_id))
    }

    pub(crate) async fn credit_hourly_income(&self, user_id: &str) -> Result<(Money, Resources)> {
        let response = self.list_credits(&CreditUserId(user_id.to_string())).await?;

        let mut money = Money::default();
        let mut resources = Resources::default();
        for credit in response.credits {
            if credit.credit_type.is_money() {
                money = Money::from(credit.hourly);
            } else {
                resources.insert(credit.credit_type.into_resource_name(), credit.hourly);
            }
        }

        Ok((money, resources))
    }

    pub(crate) async fn money_balance(&self, user_id: &CreditUserId) -> Result<MoneyBalance> {
        let response = self.list_credits(user_id).await?;
        Ok(response
            .credits
            .into_iter()
            .find(|credit| credit.credit_type.is_money())
            .map_or_else(MoneyBalance::default, |credit| MoneyBalance {
                balance: Money::from(credit.balance),
                hourly: Money::from(credit.hourly),
            }))
    }

    pub(crate) async fn subscriptions(
        &self,
        user_id: &CreditUserId,
        name_mappings: &NameMappings,
    ) -> Result<AccountSubscriptions> {
        let response = self.list_subscriptions(user_id).await?;
        let mut outgoing = Vec::new();
        let mut incoming = Vec::new();
        for subscription in response.subscriptions {
            let is_outgoing = subscription.sender == *user_id;
            let is_incoming = subscription.receiver == *user_id;
            let subscription = Subscription {
                id: subscription.id,
                sender: CreditUserName::resolve(subscription.sender, name_mappings)?,
                receiver: CreditUserName::resolve(subscription.receiver, name_mappings)?,
                value: subscription.value,
                subscription_type: subscription.subscription_type,
                priority: subscription.priority,
                credit_type: subscription.credit_type,
            };
            if is_outgoing {
                outgoing.push(subscription.clone());
            }
            if is_incoming {
                incoming.push(subscription);
            }
        }

        Ok(AccountSubscriptions { outgoing, incoming })
    }

    pub(crate) async fn account(&self, user_id: &CreditUserId, name_mappings: &NameMappings) -> Result<CreditAccount> {
        let (credits, subscriptions) =
            tokio::try_join!(self.list_credits(user_id), self.subscriptions(user_id, name_mappings))?;
        let mut money = MoneyBalance::default();
        let mut resources = HashMap::new();
        for credit in credits.credits {
            if credit.credit_type.is_money() {
                money = MoneyBalance {
                    balance: Money::from(credit.balance),
                    hourly: Money::from(credit.hourly),
                };
            } else {
                resources.insert(
                    credit.credit_type.into_resource_name(),
                    ResourceBalance {
                        balance: credit.balance,
                        hourly: credit.hourly,
                    },
                );
            }
        }

        Ok(CreditAccount {
            money,
            resources,
            subscriptions,
        })
    }

    async fn list_credits(&self, user_id: &CreditUserId) -> Result<ListCreditsResponse> {
        let url = self.endpoint(&format!("api/users/{}/credits", user_id.as_str()))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("requesting credits for credit-exchanger user {user_id}"))?;
        let response = Self::error_for_status(
            response,
            &format!("requesting credits for credit-exchanger user {user_id}"),
        )
        .await?;
        response
            .json()
            .await
            .with_context(|| format!("decoding credits for credit-exchanger user {user_id}"))
    }

    async fn list_subscriptions(&self, user_id: &CreditUserId) -> Result<ListSubscriptionsResponse> {
        let url = self.endpoint(&format!("api/users/{}/subscriptions", user_id.as_str()))?;
        let response = self
            .client
            .get(url)
            .send()
            .await
            .with_context(|| format!("listing subscriptions for credit-exchanger user {user_id}"))?;
        let response = Self::error_for_status(
            response,
            &format!("listing subscriptions for credit-exchanger user {user_id}"),
        )
        .await?;
        response
            .json()
            .await
            .with_context(|| format!("decoding subscriptions for credit-exchanger user {user_id}"))
    }

    async fn register_producer(&self, producer: &CreditUserId, policy: &Financiers) -> Result<()> {
        self.ensure_unit_user(producer).await?;
        self.create_financed_subscriptions(policy, producer).await
    }

    async fn ensure_unit_user(&self, id: &CreditUserId) -> Result<()> {
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

    async fn create_financed_subscriptions(&self, policy: &Financiers, producer: &CreditUserId) -> Result<()> {
        for subscription in financed_subscription_specs(policy, &self.resources) {
            self.create_subscription(
                producer,
                &subscription.receiver,
                &subscription.credit_type,
                subscription.share,
            )
            .await?;
        }
        Ok(())
    }

    async fn create_subscription(
        &self,
        producer: &CreditUserId,
        receiver: &str,
        credit_type: &str,
        share: Share,
    ) -> Result<()> {
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

    async fn delete_user_subscriptions(&self, user_id: &CreditUserId) -> Result<()> {
        let subscriptions = match self.list_subscriptions(user_id).await {
            Ok(subscriptions) => subscriptions,
            Err(error)
                if error
                    .downcast_ref::<CreditExchangeResponseError>()
                    .is_some_and(|response| response.status() == StatusCode::NOT_FOUND) =>
            {
                return Ok(());
            }
            Err(error) => return Err(error),
        };

        for subscription in subscriptions.subscriptions {
            let url = self.endpoint(&format!(
                "api/users/{user_id}/subscriptions/{subscription_id}",
                subscription_id = subscription.id.0,
            ))?;
            let response = self.client.delete(url).send().await.with_context(|| {
                format!(
                    "deleting subscription {subscription_id} for credit-exchanger user {user_id}",
                    subscription_id = subscription.id.0,
                )
            })?;
            if response.status() == StatusCode::NOT_FOUND {
                continue;
            }
            Self::error_for_status(
                response,
                &format!(
                    "deleting subscription {subscription_id} for credit-exchanger user {user_id}",
                    subscription_id = subscription.id.0,
                ),
            )
            .await?;
        }

        Ok(())
    }

    async fn set_credit_production(&self, user_id: &CreditUserId, value: Money) -> Result<()> {
        self.set_production(user_id, "money", value.value()).await
    }

    async fn set_resource_production(&self, user_id: &CreditUserId, resource: &ResourceName, value: f32) -> Result<()> {
        self.set_production(user_id, resource.as_str(), value).await
    }

    async fn set_production(&self, user_id: &CreditUserId, credit_type: &str, value: f32) -> Result<()> {
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
        log::warn!("{action} failed with {status}: {body}");
        Err(CreditExchangeResponseError::new(status, body).into())
    }

    fn base_credit_user_id(id: BaseId) -> CreditUserId {
        CreditUserId(format!("base-{}", id.0))
    }

    fn trust_credit_user_id(id: TrustId) -> CreditUserId {
        CreditUserId(format!("trust-{}", id.0))
    }

    fn production_unit_credit_user_id(key: &ProductionUnitKey) -> CreditUserId {
        CreditUserId(format!("trust-{key}"))
    }
}

fn sum_resource_totals(users: Vec<CreditUserResponse>, bank_user_id: &str) -> Resources {
    let mut totals = Resources::default();
    for user in users.into_iter().filter(|user| user.id != bank_user_id) {
        for (resource, credit) in user.resources {
            let total = totals.get(&resource).unwrap_or_default() + credit.total;
            totals.insert(resource, total);
        }
    }
    totals
}

fn financed_subscription_specs(policy: &Financiers, resources: &VecResourceName) -> Vec<SubscriptionSpec> {
    let mut subscriptions = policy
        .payers()
        .map(|payer| SubscriptionSpec {
            receiver: payer.payer_id,
            credit_type: "money".to_string(),
            share: payer.share,
        })
        .collect::<Vec<_>>();
    subscriptions.extend(resources.iter().map(|resource| SubscriptionSpec {
        receiver: policy.primary_payer_id().to_string(),
        credit_type: resource.as_str().to_string(),
        share: Share::from(1.0),
    }));
    subscriptions
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use actix_web::{App, HttpResponse, HttpServer, web};

    use super::*;
    use crate::{handlers::bases::Financing, services::credit_exchange_service::Share};

    fn test_service(url: Url) -> CreditExchangeService {
        CreditExchangeService::new(
            url,
            "bank".to_string(),
            serde_json::from_value(serde_json::json!({ "money": 0.0, "resources": {} })).unwrap(),
            serde_json::from_value(serde_json::json!({ "money": 0.0, "resources": {} })).unwrap(),
            serde_json::from_value(serde_json::json!({ "money": 0.0, "resources": {} })).unwrap(),
            serde_json::from_value(serde_json::json!([])).unwrap(),
            LootFactors::default(),
        )
    }

    #[test]
    fn credit_user_names_resolve_from_configured_mappings() {
        let mappings = NameMappings::new(
            HashMap::from([(
                BlocKey::from("bloc-key".to_string()),
                BlocName::from("Bloc Name".to_string()),
            )]),
            HashMap::from([(
                ZoneKey::from("zone-key".to_string()),
                ZoneName::from("Zone Name".to_string()),
            )]),
            HashMap::from([(
                CharacterKey::from("character-key".to_string()),
                CharacterName::from("Character Name".to_string()),
            )]),
        );

        for (id, expected) in [
            (
                "character-key",
                serde_json::json!({ "type": "character", "name": "Character Name" }),
            ),
            ("bloc-key", serde_json::json!({ "type": "bloc", "name": "Bloc Name" })),
            ("zone-key", serde_json::json!({ "type": "zone", "name": "Zone Name" })),
            (
                "unknown-key",
                serde_json::json!({ "type": "other", "id": "unknown-key" }),
            ),
        ] {
            let name = CreditUserName::resolve(CreditUserId(id.to_string()), &mappings).unwrap();
            assert_eq!(serde_json::to_value(name).unwrap(), expected);
        }
    }

    #[test]
    fn ambiguous_credit_user_id_is_rejected() {
        let mappings = NameMappings::new(
            HashMap::from([(
                BlocKey::from("shared-key".to_string()),
                BlocName::from("Bloc Name".to_string()),
            )]),
            HashMap::new(),
            HashMap::from([(
                CharacterKey::from("shared-key".to_string()),
                CharacterName::from("Character Name".to_string()),
            )]),
        );

        let error = CreditUserName::resolve(CreditUserId("shared-key".to_string()), &mappings).unwrap_err();

        assert_eq!(
            error.to_string(),
            "credit user ID shared-key matches multiple configured names"
        );
    }

    #[test]
    fn financed_subscriptions_split_only_money() {
        let policy = Financiers::new(
            "primary".to_string(),
            vec![Financing {
                financier: "financier".to_string().into(),
                share: Share::from(0.4),
            }],
        )
        .unwrap();
        let resources = serde_json::from_str::<VecResourceName>(r#"["iron", "copper"]"#).unwrap();

        assert_eq!(
            financed_subscription_specs(&policy, &resources),
            vec![
                SubscriptionSpec {
                    receiver: "primary".to_string(),
                    credit_type: "money".to_string(),
                    share: Share::from(0.6),
                },
                SubscriptionSpec {
                    receiver: "financier".to_string(),
                    credit_type: "money".to_string(),
                    share: Share::from(0.4),
                },
                SubscriptionSpec {
                    receiver: "primary".to_string(),
                    credit_type: "iron".to_string(),
                    share: Share::from(1.0),
                },
                SubscriptionSpec {
                    receiver: "primary".to_string(),
                    credit_type: "copper".to_string(),
                    share: Share::from(1.0),
                },
            ]
        );
    }

    #[test]
    fn production_units_use_trust_credit_user_ids() {
        let key = serde_json::from_str::<ProductionUnitKey>(r#""humans-0-0-e""#).unwrap();

        assert_eq!(
            CreditExchangeService::production_unit_credit_user_id(&key).to_string(),
            "trust-humans-0-0-e"
        );
    }

    #[test]
    fn resource_totals_exclude_bank_and_sum_every_other_user() {
        let users = serde_json::from_value::<Vec<CreditUserResponse>>(serde_json::json!([
            {
                "id": "bank",
                "userType": "bloc",
                "resources": { "iron": { "total": 1000.0 } }
            },
            {
                "id": "zone-1",
                "userType": "zone",
                "resources": { "iron": { "total": 2.5 }, "water": { "total": 4.0 } }
            },
            {
                "id": "zone-2",
                "userType": "zone",
                "resources": { "iron": { "total": 3.5 } }
            },
            {
                "id": "individual",
                "userType": "individual",
                "credit": { "total": 10.0 }
            }
        ]))
        .unwrap();

        let totals = sum_resource_totals(users, "bank");

        assert_eq!(totals.get(&ResourceName::new("iron".to_string())), Some(6.0));
        assert_eq!(totals.get(&ResourceName::new("water".to_string())), Some(4.0));
    }

    #[actix_web::test]
    async fn deleting_user_subscriptions_lists_and_deletes_every_subscription() {
        let deleted = Arc::new(Mutex::new(Vec::<String>::new()));
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server_deleted = deleted.clone();
        let server = HttpServer::new(move || {
            App::new()
                .app_data(web::Data::new(server_deleted.clone()))
                .route(
                    "/api/users/base-7/subscriptions",
                    web::get().to(|| async {
                        HttpResponse::Ok().json(serde_json::json!({
                            "subscriptions": [
                                {
                                    "id": "first",
                                    "sender": "base-7",
                                    "receiver": "receiver",
                                    "value": 1.0,
                                    "subscriptionType": "contract",
                                    "priority": 0,
                                    "creditType": "money"
                                },
                                {
                                    "id": "second",
                                    "sender": "base-7",
                                    "receiver": "receiver",
                                    "value": 2.0,
                                    "subscriptionType": "sr",
                                    "priority": 1,
                                    "creditType": "iron"
                                }
                            ]
                        }))
                    }),
                )
                .route(
                    "/api/users/base-7/subscriptions/{subscription_id}",
                    web::delete().to(
                        |path: web::Path<String>, deleted: web::Data<Arc<Mutex<Vec<String>>>>| async move {
                            deleted.lock().unwrap().push(path.into_inner());
                            HttpResponse::Ok().finish()
                        },
                    ),
                )
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        test_service(format!("http://{address}/").parse().unwrap())
            .delete_base_subscriptions(BaseId(7))
            .await
            .unwrap();

        assert_eq!(*deleted.lock().unwrap(), ["first", "second"]);
        handle.stop(true).await;
    }

    #[actix_web::test]
    async fn missing_credit_user_is_treated_as_already_clean() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = HttpServer::new(|| {
            App::new().route(
                "/api/users/trust-3/subscriptions",
                web::get().to(|| async { HttpResponse::NotFound().finish() }),
            )
        })
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);

        test_service(format!("http://{address}/").parse().unwrap())
            .delete_trust_subscriptions(TrustId(3))
            .await
            .unwrap();

        handle.stop(true).await;
    }
}
