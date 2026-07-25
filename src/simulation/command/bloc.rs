use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{Bloc, BlocName, Chance},
    services::credit_exchange_service::Share,
};

pub(crate) async fn get_all(resp: Sender<Vec<Bloc>>, blocs: &HashMap<BlocName, Arc<RwLock<Bloc>>>) {
    let mut out = Vec::with_capacity(blocs.len());
    for bloc in blocs.values() {
        out.push(bloc.read().await.clone());
    }
    let _ = resp.send(out);
}

pub(crate) fn patch(current: &Bloc, chance: Option<Chance>, military_expense: Option<Share>) -> Bloc {
    let new_chance = chance.unwrap_or_else(|| current.chance());
    let new_military_expense = military_expense.unwrap_or_else(|| current.military_expense());
    Bloc::new(
        current.name().clone(),
        current.display_name().to_owned(),
        new_chance,
        new_military_expense,
    )
}
