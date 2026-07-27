use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use tokio::sync::RwLock;

use crate::{
    domain::{BaseId, Bloc, BlocKey, MilitaryBase, MilitaryUnit, UnitId},
    geometry::{Point, Positioned},
    services::credit_exchange_service::CreditExchangeService,
};

async fn create(
    base: Arc<RwLock<MilitaryBase>>,
    position: Point,
    credit_exchange_service: &CreditExchangeService,
) -> Result<MilitaryUnit> {
    let base_guard = base.read().await;
    let bloc = base_guard.bloc_key();
    let payment = credit_exchange_service.pay_for_military_unit(bloc).await?;
    drop(base_guard);
    let unit = MilitaryUnit::new(payment, credit_exchange_service.loot_factors(), base, position);
    Ok(unit)
}

/// Runs one hourly production cycle: for each bloc, uses the configured military expense
/// percentage of the bloc's hourly income to create units at enabled bases.
///
/// Enabled bases are processed in ascending id order. Prioritised bases produce 2 units per
/// pass, regular enabled bases 1. After iterating all bases, the cycle restarts from the
/// beginning until the budget (money and resources) is exhausted.
pub(crate) async fn produce_units(
    blocs: &HashMap<BlocKey, Arc<RwLock<Bloc>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    credit_exchange_service: &CreditExchangeService,
) -> Result<()> {
    let unit_money_cost = credit_exchange_service.military_unit.money();
    let unit_resource_cost = credit_exchange_service.military_unit.resources_owned();

    for (bloc_name, bloc_arc) in blocs {
        let military_expense = {
            let bloc = bloc_arc.read().await;
            bloc.military_expense()
        };
        if military_expense == Default::default() {
            log::info!("Bloc \"{bloc_name}\" has no military expense, no units are produced.");
            continue;
        }

        let (hourly_money, hourly_resources) = credit_exchange_service.hourly_income(bloc_name).await;
        let mut budget_money = military_expense * hourly_money;
        let mut budget_resources = military_expense * hourly_resources;

        // Collect enabled bases for this bloc sorted by id ascending.
        let mut enabled_bases_with_quota: Vec<(BaseId, Arc<RwLock<MilitaryBase>>, u32)> = Vec::new();
        for base_arc in bases.values() {
            let base = base_arc.read().await;
            if !base.enabled() || base.bloc_key() != bloc_name {
                continue;
            }
            // Prioritized bases produce 2 units per pass, non-prioritized produce 1 unit per pass.
            let quota = if base.prioritized() { 2u32 } else { 1u32 };
            let id = base.id();
            drop(base);
            enabled_bases_with_quota.push((id, base_arc.clone(), quota));
        }

        if enabled_bases_with_quota.is_empty() {
            log::info!("Bloc \"{bloc_name}\" has no enabled bases, no units are produced.");
            continue;
        }

        log::info!(
            "Bloc {bloc_name}: hourly income {hourly_money}, production budget {budget_money} {budget_resources}"
        );

        // Round Robin spending of the budget, prioritized bases first, ascending ids.
        enabled_bases_with_quota.sort_by_key(|(id, ..)| *id);
        let mut produced_units = 0;
        'outer: loop {
            for (_, base_arc, quota) in &enabled_bases_with_quota {
                for _ in 0..*quota {
                    if budget_money < unit_money_cost || !budget_resources.covers(&unit_resource_cost) {
                        break 'outer;
                    }
                    let base = base_arc.read().await;
                    let position = base.position();
                    let base_id = base.id();
                    drop(base);
                    let unit = create(base_arc.clone(), position, credit_exchange_service).await?;
                    let unit_id = unit.id();
                    units.insert(unit_id, Arc::new(RwLock::new(unit)));
                    produced_units += 1;
                    log::info!("added unit {unit_id:?} to base {base_id:?}");
                    budget_money -= unit_money_cost;
                    budget_resources -= &unit_resource_cost;
                }
            }
        }
        log::info!("Bloc {bloc_name}: produced {produced_units} units");
    }
    Ok(())
}
