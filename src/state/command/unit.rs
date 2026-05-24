use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{BaseId, Bloc, BlocName, MilitaryBase, MilitaryUnit, Target, Trust, TrustId, UnitId},
    geometry::{Distance, Point, Positioned},
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
/// Enabled bases are processed in ascending id order. Prioritised bases produce 2 units per
/// pass, regular enabled bases 1. After iterating all bases, the cycle restarts from the
/// beginning until the budget (money and resources) is exhausted.
pub(crate) async fn produce_units(
    blocs: &HashMap<BlocName, Arc<RwLock<Bloc>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    payment_service: &PaymentService,
) {
    let unit_money_cost = payment_service.military_unit.money();
    let unit_resource_cost = payment_service.military_unit.resources_owned();

    for (bloc_name, bloc_arc) in blocs {
        let military_expense = {
            let bloc = bloc_arc.read().await;
            bloc.military_expense()
        };
        if military_expense == Default::default() {
            continue;
        }

        let (hourly_money, hourly_resources) = payment_service.hourly_income(bloc_name).await;
        let mut budget_money = military_expense * hourly_money;
        let mut budget_resources = military_expense * hourly_resources;

        log::info!("Bloc {bloc_name}: hourly income {hourly_money}, production budget {budget_money}");

        // Collect enabled bases for this bloc sorted by id ascending.
        let mut enabled_bases_with_quota: Vec<(BaseId, Arc<RwLock<MilitaryBase>>, u32)> = Vec::new();
        for base_arc in bases.values() {
            let base = base_arc.read().await;
            if !base.enabled() || base.placement().zone().bloc().name() != bloc_name {
                continue;
            }
            // Prioritized bases produce 2 units per pass, non-prioritized produce 1 unit per pass.
            let quota = if base.prioritized() { 2u32 } else { 1u32 };
            let id = base.id();
            drop(base);
            enabled_bases_with_quota.push((id, base_arc.clone(), quota));
        }

        if enabled_bases_with_quota.is_empty() {
            continue;
        }

        // Round Robin spending of the budget, prioritized bases first, ascending ids.
        enabled_bases_with_quota.sort_by_key(|(id, ..)| *id);
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
                    let unit = create(base_arc.clone(), position, payment_service);
                    let unit_id = unit.id().clone();
                    units.insert(unit_id.clone(), Arc::new(RwLock::new(unit)));
                    log::info!("added unit {unit_id:?} to base {base_id:?}");
                    budget_money -= unit_money_cost;
                    budget_resources -= &unit_resource_cost;
                }
            }
        }
    }
}

/// Moves `unit` one `step` toward the closest target in `targets`. Does nothing if `targets` is empty.
pub(crate) fn move_toward_closest(unit: &mut MilitaryUnit, targets: &[&dyn Positioned], step: Distance) {
    let Some(closest) = targets.iter().min_by(|a, b| {
        let da = unit.distance_to(&a.position());
        let db = unit.distance_to(&b.position());
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    }) else {
        return;
    };
    unit.move_toward(closest.position(), step);
}

/// Runs one movement tick: each unit moves one `step` toward the closest enemy target.
///
/// The target type (trust, base, or unit) is determined by the `Target` set on each unit's home base.
/// Enemy entities are those belonging to a different bloc than the unit's own bloc.
/// Pre-collects all positions before mutating units to avoid borrow conflicts.
pub(crate) async fn move_units(
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    step: Distance,
) {
    // Pre-collect (BlocName, Point) for trusts and bases.
    let mut trust_positions: Vec<(BlocName, Point)> = Vec::with_capacity(trusts.len());
    for trust_arc in trusts.values() {
        let trust = trust_arc.read().await;
        let bloc_name = trust.placement().zone().bloc().name().clone();
        trust_positions.push((bloc_name, trust.position()));
    }

    let mut base_positions: Vec<(BlocName, Point)> = Vec::with_capacity(bases.len());
    for base_arc in bases.values() {
        let base = base_arc.read().await;
        let bloc_name = base.placement().zone().bloc().name().clone();
        base_positions.push((bloc_name, base.position()));
    }

    // Pre-collect (BlocName, UnitId, Point) for units.
    let mut unit_positions: Vec<(BlocName, UnitId, Point)> = Vec::with_capacity(units.len());
    for (unit_id, unit_arc) in units.iter() {
        let unit = unit_arc.read().await;
        let base = unit.base().await;
        let bloc_name = base.placement().zone().bloc().name().clone();
        unit_positions.push((bloc_name, unit_id.clone(), unit.position()));
    }

    for (unit_id, unit_arc) in units.iter() {
        let mut unit = unit_arc.write().await;
        let (unit_bloc, target) = {
            let base = unit.base().await;
            (base.placement().zone().bloc().name().clone(), base.target())
        };

        let enemy_points: Vec<Point> = match target {
            Target::Trust => trust_positions
                .iter()
                .filter(|(bloc, _)| bloc != &unit_bloc)
                .map(|(_, pt)| *pt)
                .collect(),
            Target::Base => base_positions
                .iter()
                .filter(|(bloc, _)| bloc != &unit_bloc)
                .map(|(_, pt)| *pt)
                .collect(),
            Target::Unit => unit_positions
                .iter()
                .filter(|(bloc, id, _)| bloc != &unit_bloc && id != unit_id)
                .map(|(_, _, pt)| *pt)
                .collect(),
        };

        let positioned: Vec<&dyn Positioned> = enemy_points.iter().map(|p| p as &dyn Positioned).collect();
        move_toward_closest(&mut unit, &positioned, step);
    }
}
