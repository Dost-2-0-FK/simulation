use serde::{Deserialize, Serialize};

use crate::services::credit_exchange_service::{
    Cost, Money, ResourceName, ResourceValue, Resources, ResourcesFactors, Share,
};

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
    pub(crate) fn new(money: Money, resources: Resources) -> Self {
        Self { money, resources }
    }

    pub(crate) fn from_cost<T>(cost: &Cost<T>, factors: &LootFactors) -> Self {
        Self {
            money: cost.money() * factors.money,
            resources: cost.resources_owned() * &factors.resources,
        }
    }

    pub(crate) fn from_cost_share<T>(cost: &Cost<T>, share: Share) -> Self {
        Self {
            money: share * cost.money(),
            resources: share * cost.resources_owned(),
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

#[cfg(test)]
mod tests {
    use super::Loot;
    use crate::{
        domain::Trust,
        services::credit_exchange_service::{Cost, Share},
    };

    #[test]
    fn cost_share_scales_money_and_resources() {
        let cost = serde_json::from_value::<Cost<Trust>>(serde_json::json!({
            "money": 12.0,
            "resources": { "iron": 8.0 }
        }))
        .unwrap();

        let share_cost = Loot::from_cost_share(&cost, Share::from(0.25));

        assert_eq!(
            serde_json::to_value(share_cost).unwrap(),
            serde_json::json!({
                "money": 3.0,
                "resources": { "iron": 2.0 }
            })
        );
    }
}
