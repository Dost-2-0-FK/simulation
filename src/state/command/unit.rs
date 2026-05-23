use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{BaseId, Bloc, BlocName, MilitaryBase, MilitaryUnit, UnitId},
    geometry::{Point, Positioned},
    handlers::units::UnitResponse,
    services::payment_service::PaymentService,
};

pub(crate) async fn get(resp: Sender<Vec<UnitResponse>>, units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>) {
    let unit_responses = stream::iter(units.values())
        .then(async |unit| {
            let unit_guard = unit.read().await;
            let base_guard = unit_guard.base().await;
            let base_response = (&(*base_guard)).into();
            UnitResponse::new(&unit_guard, Some(base_response))
        })
        .collect()
        .await;
    let _ = resp.send(unit_responses);
}

pub(crate) fn create(
    base: Arc<RwLock<MilitaryBase>>,
    position: Point,
    payment_service: &PaymentService,
) -> MilitaryUnit {
    let payment = payment_service.pay_for_military_unit();
    MilitaryUnit::new(payment, base, position)
}

/// Runs one hourly production cycle: for each bloc, uses the configured military expense
/// percentage of the bloc's hourly income to create units at enabled bases.
///
/// Prioritised enabled bases produce one unit at cost 2 (processed first), regular enabled bases
/// produce one unit at cost 1. Disabled bases are skipped.
pub(crate) async fn produce_units(
    blocs: &HashMap<BlocName, Arc<RwLock<Bloc>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    payment_service: &PaymentService,
) {
    for (bloc_name, bloc_arc) in blocs {
        let military_expense = {
            let bloc = bloc_arc.read().await;
            bloc.military_expense()
        };
        if military_expense == 0 {
            continue;
        }

        let hourly_income = payment_service.hourly_income(bloc_name).await;
        let mut budget = hourly_income * military_expense as f64 / 100.0;
        log::info!("Bloc {bloc_name}: hourly income {hourly_income}, production budget {budget}");

        // Collect enabled bases for this bloc, prioritised separately from normal ones.
        let mut prioritized: Vec<Arc<RwLock<MilitaryBase>>> = Vec::new();
        let mut normal: Vec<Arc<RwLock<MilitaryBase>>> = Vec::new();

        for base_arc in bases.values() {
            let base = base_arc.read().await;
            let enabled = base.enabled();
            let bloc_matches = base.placement().zone().bloc().name() == bloc_name;
            let is_prioritized = base.prioritized();
            drop(base);

            if !enabled || !bloc_matches {
                continue;
            }
            if is_prioritized {
                prioritized.push(base_arc.clone());
            } else {
                normal.push(base_arc.clone());
            }
        }

        // Process prioritised bases first (cost 2), then normal bases (cost 1).
        let candidates = prioritized
            .iter()
            .map(|b| (b, 2.0_f64))
            .chain(normal.iter().map(|b| (b, 1.0_f64)));

        for (base_arc, cost) in candidates {
            while budget >= cost {
                let base = base_arc.read().await;
                let position = base.position();
                let base_id = base.id();
                let unit = create(base_arc.clone(), position, payment_service);
                let unit_id = unit.id().clone();
                units.insert(unit.id().clone(), Arc::new(RwLock::new(unit)));
                log::info!(
                    "added unit {unit_id:?} to base {base_id:?}",
                    unit_id = unit_id,
                    base_id = base_id,
                );
                budget -= cost;
            }
        }
    }
}
