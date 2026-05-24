use std::sync::Arc;

use derive_more::Display;
use serde::{Deserialize, Serialize};

use crate::services::payment_service::Share;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ZoneName(String);

#[derive(Debug)]
pub(crate) struct Zone(ZoneName, Arc<Bloc>);

impl Zone {
    pub(crate) fn new(name: ZoneName, bloc: Arc<Bloc>) -> Self {
        Self(name, bloc)
    }

    pub(crate) fn name(&self) -> &ZoneName {
        &self.0
    }

    pub(crate) fn bloc(&self) -> &Bloc {
        &self.1
    }
}

/// Every unit belongs to a [Zone], which belongs to a [Bloc], which implies a [Chance].
/// When two units of a different [Bloc] meet, they fight: For each unit, a die is rolled, i.e., a  uniform random draw
/// of [0, [Chance]]. On 0, the other unit is eliminated. If both dice show 0, both units are eliminated.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct Chance(f32);

impl Chance {
    pub(crate) fn new(value: f32) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, Display, utoipa::ToSchema)]
pub(crate) struct BlocName(String);

impl From<String> for BlocName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Bloc {
    name: BlocName,
    chance: Chance,
    military_expense: Share,
}

impl Bloc {
    pub(crate) fn new(name: BlocName, chance: Chance, military_expense: Share) -> Self {
        Self {
            name,
            chance,
            military_expense,
        }
    }

    pub(crate) fn name(&self) -> &BlocName {
        &self.name
    }

    pub(crate) fn chance(&self) -> Chance {
        self.chance
    }

    pub(crate) fn military_expense(&self) -> Share {
        self.military_expense
    }

    #[expect(dead_code)]
    pub(crate) fn set_chance(&mut self, chance: Chance) {
        self.chance = chance;
    }

    #[expect(dead_code)]
    pub(crate) fn set_military_expense(&mut self, military_expense: Share) {
        self.military_expense = military_expense;
    }
}
