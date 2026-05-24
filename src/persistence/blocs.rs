use serde::{Deserialize, Serialize};

use crate::{
    domain::{Bloc, BlocName, Chance},
    services::payment_service::Share,
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct PersistedBloc {
    #[serde(rename = "_id")]
    id: String,
    chance: Chance,
    military_expense: Share,
}

impl PersistedBloc {
    pub(super) fn from_bloc(bloc: &Bloc) -> Self {
        Self {
            id: bloc.name().to_string(),
            chance: bloc.chance(),
            military_expense: bloc.military_expense(),
        }
    }

    pub(super) fn into_bloc(self) -> Bloc {
        Bloc::new(BlocName::from(self.id), self.chance, self.military_expense)
    }

    pub(crate) fn id(&self) -> &str {
        &self.id
    }
}
