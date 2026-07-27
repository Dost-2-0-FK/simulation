use serde::{Deserialize, Serialize};

use crate::services::credit_exchange_service::{Money, ResourceName, ResourceValue, Resources, Share};

/// The mutable production output shared by resource-producing structures.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct Production {
    base_income: Money,
    producing: Resources,
}

impl Production {
    pub(crate) fn new(resource: ResourceName, resource_amount: f32, base_income: Money) -> Self {
        Self {
            base_income,
            producing: Resources::new_single(resource, resource_amount),
        }
    }

    pub(crate) fn from_parts(base_income: Money, producing: Resources) -> Self {
        Self { base_income, producing }
    }

    pub(crate) fn base_income(&self) -> Money {
        self.base_income
    }

    pub(crate) fn resource_name(&self) -> &ResourceName {
        self.producing.single_resource_name()
    }

    pub(crate) fn producing_base_value(&self) -> &Resources {
        &self.producing
    }

    pub(crate) fn with_factor(&self, factor: Share) -> Resources {
        factor * self.producing.clone()
    }

    pub(crate) fn income(&self, produced: ResourceValue<'_>, existing_resource_units: f32) -> Money {
        produced * self.base_income / (existing_resource_units + 1.0)
    }
}
