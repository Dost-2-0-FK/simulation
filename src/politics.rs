use std::sync::Arc;

use derive_more::Display;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq, Eq, derive_more::Display)]
pub(crate) struct ZoneName(String);

#[derive(Debug)]
#[expect(dead_code)]
pub(crate) struct Zone(ZoneName, Arc<Bloc>);

impl Zone {
    pub(crate) fn new(name: ZoneName, bloc: Arc<Bloc>) -> Self {
        Self(name, bloc)
    }

    pub(crate) fn name(&self) -> &ZoneName {
        &self.0
    }
}

/// Every unit belongs to a [Zone], which belongs to a [Bloc], which implies a [Chance].
/// When two units of a different [Bloc] meet, they fight: For each unit, a die is rolled, i.e., a  uniform random draw
/// of [0, [Chance]]. On 0, the other unit is eliminated. If both dice show 0, both units are eliminated.
#[derive(Debug, Clone, Deserialize)]
#[expect(dead_code)]
pub(crate) struct Chance(u32);

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Display)]
pub(crate) struct BlocName(String);

#[derive(Debug)]
#[expect(dead_code)]
pub(crate) struct Bloc(BlocName, Chance);

impl Bloc {
    pub(crate) fn new(name: BlocName, chance: Chance) -> Self {
        Self(name, chance)
    }

    pub(crate) fn name(&self) -> &BlocName {
        &self.0
    }
}
