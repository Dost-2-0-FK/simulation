use std::sync::Arc;

use derive_more::Display;
use rand::RngExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::services::credit_exchange_service::Share;

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ZoneName(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ZoneKey(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct BlocKey(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct CharacterKey(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct CharacterName(String);

macro_rules! string_identifier {
    ($type:ty) => {
        impl From<String> for $type {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<$type> for String {
            fn from(value: $type) -> Self {
                value.0
            }
        }

    };
}

string_identifier!(ZoneName);
string_identifier!(ZoneKey);
string_identifier!(BlocName);
string_identifier!(BlocKey);
string_identifier!(CharacterKey);
string_identifier!(CharacterName);

impl BlocKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl CharacterKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub(crate) struct Zone {
    key: ZoneKey,
    name: ZoneName,
    bloc_key: BlocKey,
    bloc_name: BlocName,
    bloc: Arc<RwLock<Bloc>>,
}

impl Zone {
    pub(crate) fn new(
        key: ZoneKey,
        name: ZoneName,
        bloc_key: BlocKey,
        bloc_name: BlocName,
        bloc: Arc<RwLock<Bloc>>,
    ) -> Self {
        Self {
            key,
            name,
            bloc_key,
            bloc_name,
            bloc,
        }
    }

    pub(crate) fn key(&self) -> &ZoneKey {
        &self.key
    }

    pub(crate) fn name(&self) -> &ZoneName {
        &self.name
    }

    pub(crate) fn bloc_name(&self) -> &BlocName {
        &self.bloc_name
    }

    pub(crate) fn bloc_key(&self) -> &BlocKey {
        &self.bloc_key
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

#[derive(Debug, Clone)]
pub(crate) struct Bloc {
    key: BlocKey,
    name: BlocName,
    chance: Chance,
    military_expense: Share,
}

impl Bloc {
    pub(crate) fn new(key: BlocKey, name: BlocName, chance: Chance, military_expense: Share) -> Self {
        Self {
            key,
            name,
            chance,
            military_expense,
        }
    }

    pub(crate) fn name(&self) -> &BlocName {
        &self.name
    }

    pub(crate) fn key(&self) -> &BlocKey {
        &self.key
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
