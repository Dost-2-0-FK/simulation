use std::sync::Arc;

use serde::{Deserialize, Deserializer};

use crate::military::MilitaryUnitCost;

#[derive(Debug, Copy, Clone, Deserialize)]
pub(crate) struct Money(f32);

impl Default for Money {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) struct ResourceName(String);

// Ensure that a resource name is always a lowercase string
impl<'de> Deserialize<'de> for ResourceName {
    fn deserialize<D>(d: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(d)?;
        Ok(ResourceName(s.to_lowercase()))
    }
}

#[derive(Debug, derive_more::Deref, Deserialize)]
pub(super) struct VecResourceName(Vec<ResourceName>);

impl std::fmt::Display for VecResourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();
        for entry in self.iter() {
            list.entry(&entry.0);
        }
        list.finish()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResourceValue(ResourceName, f32);

/// Produced by the payment service when a cost was paid.
#[derive(Debug)]
pub(crate) struct Payment<T>(Arc<T>);

impl<T> Payment<T> {
    pub(crate) fn cost(&self) -> &T {
        &self.0
    }
}

/// Constructed when loading the config
// TODO quite likely that in the end this works entirely different; we just need some struct to hold the costs
// to enable constructing the `CostPaid` to instantiate a `MilitaryUnit`.
pub(crate) struct Costs {
    pub(crate) military_unit: Arc<MilitaryUnitCost>,
}

impl Costs {
    pub(crate) fn pay_for_military_unit(&self) -> Payment<MilitaryUnitCost> {
        Payment(self.military_unit.clone())
    }
}
