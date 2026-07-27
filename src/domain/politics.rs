use std::sync::Arc;

use derive_more::Display;
use rand::RngExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::services::credit_exchange_service::Share;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ZoneName(String);

impl From<String> for ZoneName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug)]
pub(crate) struct Zone {
    key: ZoneName,
    name: String,
    bloc_name: BlocName,
    bloc: Arc<RwLock<Bloc>>,
}

impl Zone {
    pub(crate) fn new(key: ZoneName, name: String, bloc_name: BlocName, bloc: Arc<RwLock<Bloc>>) -> Self {
        Self {
            key,
            name,
            bloc_name,
            bloc,
        }
    }

    /// Stable key used by persistence and integrations.
    pub(crate) fn name(&self) -> &ZoneName {
        &self.key
    }

    pub(crate) fn display_name(&self) -> &str {
        &self.name
    }

    pub(crate) fn bloc_name(&self) -> &BlocName {
        &self.bloc_name
    }

    pub(crate) async fn bloc(&self) -> RwLockReadGuard<'_, Bloc> {
        self.bloc.read().await
    }
}

/// Every unit belongs to a [Zone], which belongs to a [Bloc], which implies a [Chance].
/// When two units of a different [Bloc] meet, they fight: For each unit, a die is rolled, i.e., a  uniform random draw
/// of [0, [Chance]]. On 0, the other unit is eliminated. If both dice show 0, both units are eliminated.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct Chance(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DieRollOutcome {
    Hit,
    Miss,
}

impl Chance {
    pub(crate) fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) fn roll(&self) -> DieRollOutcome {
        if rand::rng().random_range(0..=self.0 - 1) == 0 {
            return DieRollOutcome::Hit;
        }
        DieRollOutcome::Miss
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
    key: BlocName,
    name: String,
    chance: Chance,
    military_expense: Share,
}

impl Bloc {
    pub(crate) fn new(key: BlocName, name: String, chance: Chance, military_expense: Share) -> Self {
        Self {
            key,
            name,
            chance,
            military_expense,
        }
    }

    /// Stable key used by persistence, authorization, and integrations.
    pub(crate) fn name(&self) -> &BlocName {
        &self.key
    }

    pub(crate) fn display_name(&self) -> &str {
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
