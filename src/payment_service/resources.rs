use std::{borrow::Cow, collections::HashMap};

use ordered_float::OrderedFloat;
use serde::{Deserialize, Deserializer};

#[derive(Debug, Clone, Hash, PartialEq, Eq, derive_more::Display)]
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
}

/// A set of resources with corresponding amounts
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Resources(HashMap<ResourceName, OrderedFloat<f32>>);

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
