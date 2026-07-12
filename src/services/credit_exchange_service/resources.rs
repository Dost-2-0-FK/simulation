use std::{borrow::Cow, collections::HashMap};

use derive_more::Display;
use ordered_float::OrderedFloat;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Hash, PartialEq, Eq, derive_more::Display)]
pub(crate) struct ResourceName(String);

impl ResourceName {
    pub(crate) fn new(name: String) -> Self {
        Self(name.to_lowercase())
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

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
pub(crate) struct VecResourceName(Vec<ResourceName>);

impl std::fmt::Display for VecResourceName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();
        for entry in self.iter() {
            list.entry(&entry.0);
        }
        list.finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
#[display("ResourceValue({_0}, {_1})")]
pub(crate) struct ResourceValue<'a>(Cow<'a, ResourceName>, OrderedFloat<f32>);

// Decided to try without a getter to the underlying float value to enforce _always_ keeping this value bound to a
// concrete resource.
impl ResourceValue<'_> {
    pub(crate) fn name(&self) -> &ResourceName {
        &self.0
    }

    pub(crate) fn value(&self) -> f32 {
        self.1.0
    }
}

/// A set of resources with corresponding amounts
#[derive(Debug, Clone, Default, Deserialize, Display)]
#[display("{}", 
    _0.iter()
        .map(|(k, v)| format!("{k}: {}", v.0))
        .collect::<Vec<_>>()
        .join(", ")
)]
pub(crate) struct Resources(HashMap<ResourceName, OrderedFloat<f32>>);

impl Resources {
    pub(crate) fn insert(&mut self, name: ResourceName, amount: f32) {
        self.0.insert(name, OrderedFloat(amount));
    }

    /// Returns true if `self` has enough of every resource required by `cost`.
    pub(crate) fn covers(&self, cost: &Resources) -> bool {
        cost.0
            .iter()
            .all(|(name, amount)| self.0.get(name).is_some_and(|a| a >= amount))
    }

    fn expect_value(&self, name: &ResourceName) -> f32 {
        (*self.0.get(name).expect("expecting value")).into()
    }
}

impl std::ops::Mul<f32> for Resources {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self(self.0.into_iter().map(|(k, v)| (k, v * rhs)).collect())
    }
}

impl std::ops::SubAssign<&Resources> for Resources {
    fn sub_assign(&mut self, rhs: &Resources) {
        for (name, amount) in &rhs.0 {
            if let Some(a) = self.0.get_mut(name) {
                *a -= amount;
            }
        }
    }
}

impl<'r> IntoIterator for &'r Resources {
    type Item = ResourceValue<'r>;

    type IntoIter = std::iter::Map<
        std::collections::hash_map::Iter<'r, ResourceName, OrderedFloat<f32>>,
        fn((&'r ResourceName, &'r OrderedFloat<f32>)) -> ResourceValue<'r>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.0
            .iter()
            .map(|(name, value)| ResourceValue(Cow::Borrowed(name), *value))
    }
}

struct ResourcesFactors(Resources);

impl std::ops::Mul<ResourcesFactors> for Resources {
    type Output = Self;
    fn mul(self, factors: ResourcesFactors) -> Self {
        Self(
            self.0
                .into_iter()
                .map(|(name, value)| {
                    let factor = factors
                        .0
                        .0
                        .get(&name)
                        .expect("config parsing ensures that resources factors and resources have matching keys");
                    (name, value * factor)
                })
                .collect(),
        )
    }
}
