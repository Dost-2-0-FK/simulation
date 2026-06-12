use std::{collections::HashMap, sync::Arc};

use futures_util::{StreamExt, stream};
use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{BaseId, Bloc, BlocName, MilitaryBase, MilitaryUnit, Target, UnitId},
    geometry::{Distance, Point, Positioned},
    handlers::units::{UnitResponse, UnitTargetResponse},
    services::payment_service::PaymentService,
};

/// The resolved destination a unit will move toward this tick.
enum MoveTo {
    /// An enemy unit is the closest target.
    EnemyUnit(UnitId, Point),
    /// The designated secondary target (base or trust) is the closest.
    Designated(Point),
}

/// Core sync helper: given a unit's current position, its optional designated `target_point`, and
/// all unit snapshots, returns where the unit should move — or `None` when there is nothing to
/// move toward (no designated target and no enemies).
///
/// An enemy unit that is strictly closer than `target_point` always wins over the designated
/// target. When `target_point` is `None` the closest enemy unit is returned unconditionally.
fn select_move_target(
    from: Point,
    unit_id: &UnitId,
    unit_bloc: &BlocName,
    target_point: Option<Point>,
    all_units: &[(BlocName, UnitId, Point)],
) -> Option<MoveTo> {
    let enemies = || {
        all_units
            .iter()
            .filter(|(bloc, id, _)| bloc != unit_bloc && id != unit_id)
    };

    match target_point {
        None => enemies()
            .min_by(|(_, _, a), (_, _, b)| {
                from.distance_to(a)
                    .partial_cmp(&from.distance_to(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(_, id, pos)| MoveTo::EnemyUnit(id.clone(), *pos)),
        Some(tp) => {
            let target_dist = from.distance_to(&tp);
            let closer_enemy = enemies()
                .filter(|(_, _, pos)| from.distance_to(pos) < target_dist)
                .min_by(|(_, _, a), (_, _, b)| {
                    from.distance_to(a)
                        .partial_cmp(&from.distance_to(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            Some(match closer_enemy {
                Some((_, id, pos)) => MoveTo::EnemyUnit(id.clone(), *pos),
                None => MoveTo::Designated(tp),
            })
        }
    }
}

/// Computes the effective target a unit would move toward right now and returns it as a
/// `UnitTargetResponse` suitable for HTTP responses.
async fn effective_target(
    unit_id: &UnitId,
    from: Point,
    unit_bloc: &BlocName,
    target: &Target,
    all_units: &[(BlocName, UnitId, Point)],
) -> UnitTargetResponse {
    let target_point = match target {
        Target::None => None,
        Target::Base { arc, .. } => Some(arc.read().await.position()),
        Target::Trust { arc, .. } => Some(arc.read().await.position()),
    };

    match select_move_target(from, unit_id, unit_bloc, target_point, all_units) {
        None => UnitTargetResponse::None,
        Some(MoveTo::EnemyUnit(id, position)) => UnitTargetResponse::Unit { id, position },
        Some(MoveTo::Designated(position)) => match target {
            Target::Base { id, .. } => UnitTargetResponse::Base { id: *id, position },
            Target::Trust { id, .. } => UnitTargetResponse::Trust { id: *id, position },
            Target::None => panic!("Designated requires a non-None target"),
        },
    }
}

pub(crate) async fn get(resp: Sender<Vec<UnitResponse>>, units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>) {
    // Pre-collect (BlocName, UnitId, Point) for enemy-unit detection.
    let mut all_units_snapshot: Vec<(BlocName, UnitId, Point)> = Vec::with_capacity(units.len());
    for (unit_id, unit_arc) in units.iter() {
        let unit = unit_arc.read().await;
        let base = unit.base().await;
        let bloc_name = base.placement().zone().bloc().name().clone();
        all_units_snapshot.push((bloc_name, unit_id.clone(), unit.position()));
    }

    let unit_responses = stream::iter(units.values())
        .then(async |unit| {
            let unit_guard = unit.read().await;
            let base_guard = unit_guard.base().await;
            let bloc_name = base_guard.placement().zone().bloc().name().clone();
            let target = effective_target(
                unit_guard.id(),
                unit_guard.position(),
                &bloc_name,
                base_guard.target(),
                &all_units_snapshot,
            )
            .await;
            let base_response = (&(*base_guard)).into();
            UnitResponse::new(&unit_guard, Some(base_response), target)
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

/// Moves `unit` one `step` toward its effective target (see [`select_move_target`]).
fn move_toward_target_or_unit(
    unit: &mut MilitaryUnit,
    unit_bloc: &BlocName,
    target_point: Option<Point>,
    units: &[(BlocName, UnitId, Point)],
    step: Distance,
) {
    let from = unit.position();
    let move_to = match select_move_target(from, unit.id(), unit_bloc, target_point, units) {
        None => return,
        Some(MoveTo::EnemyUnit(_, pos) | MoveTo::Designated(pos)) => pos,
    };
    unit.move_toward(move_to, step);
}

/// Runs one movement tick: each unit moves one `step` toward its designated target (or the
/// closest enemy unit if one is nearer than the target).
///
/// Pre-collects all unit positions before the mutation pass to avoid borrow conflicts.
pub(crate) async fn move_units(units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>, step: Distance) {
    // Pre-collect (BlocName, UnitId, Point) for all units for enemy-unit detection.
    let mut blocs_units_points: Vec<(BlocName, UnitId, Point)> = Vec::with_capacity(units.len());
    for (unit_id, unit_arc) in units.iter() {
        let unit = unit_arc.read().await;
        let base = unit.base().await;
        let bloc_name = base.placement().zone().bloc().name().clone();
        blocs_units_points.push((bloc_name, unit_id.clone(), unit.position()));
    }

    for unit in units.values() {
        let mut unit = unit.write().await;
        let (unit_bloc, target) = {
            let base = unit.base().await;
            (base.placement().zone().bloc().name().clone(), base.target().clone())
        };

        let target_point = match &target {
            Target::None => None,
            Target::Base { arc, .. } => Some(arc.read().await.position()),
            Target::Trust { arc, .. } => Some(arc.read().await.position()),
        };

        move_toward_target_or_unit(&mut unit, &unit_bloc, target_point, &blocs_units_points, step);
    }
}
