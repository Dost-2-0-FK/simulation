use serde::{Deserialize, Serialize};

use crate::services::credit_exchange_service::{Cost, Money, ResourceName, ResourceValue, Resources, ResourcesFactors};

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct LootFactors {
    money: f32,
    resources: ResourcesFactors,
}

impl LootFactors {
    pub(crate) fn resources(&self) -> impl Iterator<Item = ResourceValue<'_>> {
        self.resources.resources()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Loot {
    money: Money,
    resources: Resources,
}

impl Loot {
    pub(crate) fn from_cost<T>(cost: &Cost<T>, factors: &LootFactors) -> Self {
        Self {
            money: cost.money() * factors.money,
            resources: cost.resources_owned() * &factors.resources,
        }
    }

    pub(crate) fn money(&self) -> Money {
        self.money
    }

    pub(crate) fn resource_amount(&self, resource: &ResourceName) -> Option<f32> {
        self.resources.get(resource)
    }

    pub(crate) fn resources(&self) -> impl Iterator<Item = ResourceValue<'_>> {
        self.resources.into_iter()
    }

    pub(crate) fn split(&self, parts: usize) -> Self {
        assert!(parts > 0, "cannot split loot into zero parts");
        let factor = 1.0 / parts as f32;
        Self {
            money: self.money * factor,
            resources: self.resources.clone() * factor,
        }
    }
}

impl std::ops::AddAssign<&Loot> for Loot {
    fn add_assign(&mut self, rhs: &Loot) {
        self.money += rhs.money;
        self.resources += &rhs.resources;
    }
}
