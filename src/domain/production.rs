use serde::{Deserialize, Serialize};

use crate::services::credit_exchange_service::{Money, ResourceName, ResourceValue, Resources, Share};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProductionFactor(f32);

impl ProductionFactor {
    pub(crate) fn new(value: f32) -> Option<Self> {
        (value.is_finite() && value >= 0.0).then_some(Self(value))
    }

    #[cfg(test)]
    pub(crate) fn value(self) -> f32 {
        self.0
    }

    pub(crate) fn combined_with(self, factor: Share) -> Self {
        Self(self.0 * factor.value())
    }
}

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

    pub(crate) fn with_factor(&self, factor: ProductionFactor) -> Resources {
        self.producing.clone() * factor.0
    }

    pub(crate) fn income(&self, produced: ResourceValue<'_>, existing_resource_units: f32) -> Money {
        produced * self.base_income / (existing_resource_units + 1.0)
    }
}
