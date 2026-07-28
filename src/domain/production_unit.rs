use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::{
    domain::{Production, Zone},
    services::credit_exchange_service::{Money, ResourceName, ResourceValue, Resources},
};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ProductionUnitKey(String);

impl ProductionUnitKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProductionUnit {
    key: ProductionUnitKey,
    zone: Arc<Zone>,
    production: Production,
}

impl ProductionUnit {
    pub(crate) fn new(
        key: ProductionUnitKey,
        zone: Arc<Zone>,
        resource: ResourceName,
        resource_amount: f32,
        base_income: Money,
    ) -> Self {
        Self {
            key,
            zone,
            production: Production::new(resource, resource_amount, base_income),
        }
    }

    pub(crate) fn from_persisted(key: ProductionUnitKey, zone: Arc<Zone>, production: Production) -> Self {
        Self { key, zone, production }
    }

    pub(crate) fn key(&self) -> &ProductionUnitKey {
        &self.key
    }

    pub(crate) fn zone(&self) -> &Zone {
        &self.zone
    }

    pub(crate) fn production(&self) -> &Production {
        &self.production
    }

    pub(crate) fn resource_name(&self) -> &ResourceName {
        self.production.resource_name()
    }

    pub(crate) async fn production_without_inhibition(&self) -> Resources {
        self.production
            .with_factor(self.zone.production_unit_factor().await)
    }

    pub(crate) fn income(&self, produced: ResourceValue<'_>, existing_resource_units: f32) -> Money {
        self.production.income(produced, existing_resource_units)
    }
}
