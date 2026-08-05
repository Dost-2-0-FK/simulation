use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{BaseId, Bloc, BlocKey, MilitaryBase, MilitaryUnit, SimulationStats, Trust, TrustId, UnitId, UnitState},
    error::UserError,
    handlers::stats::{BlocStatsResponse, StatsResponse},
    services::credit_exchange_service::CreditExchangeService,
};

pub(crate) async fn get(
    response: Sender<Result<StatsResponse, UserError>>,
    stats: &SimulationStats,
    blocs: &HashMap<BlocKey, Arc<RwLock<Bloc>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    credit_exchange_service: &CreditExchangeService,
) {
    let result = async {
        let mut bloc_responses = Vec::with_capacity(blocs.len());
        for (bloc_key, bloc) in blocs {
            let bloc_name = bloc.read().await.name().clone();

            let mut remaining_bases = 0;
            for base in bases.values() {
                if base.read().await.bloc_key() == bloc_key {
                    remaining_bases += 1;
                }
            }

            let mut remaining_trusts = 0;
            for trust in trusts.values() {
                if trust.read().await.placement().zone().bloc_key() == bloc_key {
                    remaining_trusts += 1;
                }
            }

            let mut remaining_units = 0;
            for unit in units.values() {
                let unit = unit.read().await;
                if unit.state() == UnitState::Alive && unit.base().await.bloc_key() == bloc_key {
                    remaining_units += 1;
                }
            }

            let combat_ready = if remaining_bases > 0 {
                true
            } else {
                credit_exchange_service
                    .can_afford_military_base(bloc_key)
                    .await
                    .map_err(|error| {
                        log::error!("failed to determine combat readiness for bloc {bloc_key}: {error:#}");
                        UserError::InternalError
                    })?
            };
            let bloc_stats = stats.bloc(bloc_key);
            bloc_responses.push(BlocStatsResponse::new(
                bloc_name,
                &bloc_stats,
                remaining_trusts,
                remaining_bases,
                remaining_units,
                combat_ready,
            ));
        }

        Ok(StatsResponse::new(stats.runtime_seconds(), bloc_responses))
    }
    .await;

    let _ = response.send(result);
}
