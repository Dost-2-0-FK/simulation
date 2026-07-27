use serde::{Deserialize, Serialize};

use crate::{
    domain::{Bloc, BlocKey, Chance},
    services::credit_exchange_service::Share,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PersistedBloc {
    #[serde(rename = "_id")]
    id: BlocKey,
    chance: Chance,
    military_expense: Share,
}

impl PersistedBloc {
    pub(super) fn from_bloc(bloc: &Bloc) -> Self {
        Self {
            id: bloc.key().clone(),
            chance: bloc.chance(),
            military_expense: bloc.military_expense(),
        }
    }

    pub(crate) fn key(&self) -> &BlocKey {
        &self.id
    }

    pub(crate) fn chance(&self) -> Chance {
        self.chance
    }

    pub(crate) fn military_expense(&self) -> Share {
        self.military_expense
    }

    pub(crate) fn id(&self) -> &BlocKey {
        &self.id
    }
}
