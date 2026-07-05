use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    domain::{BlocName, Combat, CombatParameters, MilitaryUnit, Target, UnitId},
    geometry::{Distance, Point, Positioned},
};

/// The resolved destination a unit will move toward this tick.
pub(super) enum MoveTo {
    /// An enemy unit is the closest target.
    EnemyUnit(UnitId, Point),
    /// The designated secondary target (base or trust) is the closest.
    Designated(Point),
    /// No appropriate target exists (e.g., when there are no enemy units)
    None,
}

/// Moves `unit` one `step` toward its effective target (see [`select_move_target`]).
/// Returns `true` if combat should be initiated, `false` otherwise.
async fn move_toward_target(
    unit: &mut MilitaryUnit,
    unit_bloc: &BlocName,
    target: &Target,
    units: &[(BlocName, UnitId, Point)],
    step: Distance,
) -> bool {
    let target_position = match target {
        Target::None => None,
        Target::Base { base, .. } => Some(base.read().await.position()),
        Target::Trust { trust, .. } => Some(trust.read().await.position()),
    };

    let from = unit.position();
    let to = match select_move_target(from, unit.id(), unit_bloc, target_position, units) {
        MoveTo::None => return false,
        MoveTo::EnemyUnit(_, position) | MoveTo::Designated(position) => position,
    };
    unit.move_toward(to, step);
    if unit.position() != to {
        return false;
    }
    true
}

/// Runs one movement tick: each unit moves one `step` toward its designated target (or the
/// closest enemy unit if one is nearer than the target).
///
/// Moved units may cause units to join an existing combat or create a new one in their position.
// Pre-collects all unit positions before the mutation pass to avoid borrow conflicts.
pub(crate) async fn move_units(
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    step: Distance,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
) {
    // Pre-collect (BlocName, UnitId, Point) for all units for enemy-unit detection.
    let mut blocs_units_points: Vec<(BlocName, UnitId, Point)> = Vec::with_capacity(units.len());
    for (unit_id, unit_arc) in units.iter() {
        let unit = unit_arc.read().await;
        let base = unit.base().await;
        let bloc_name = base.placement().zone().bloc().name().clone();
        blocs_units_points.push((bloc_name, *unit_id, unit.position()));
    }

    for unit in units.values() {
        let mut unit_write_guard = unit.write().await;
        let (unit_bloc, target) = {
            let base = unit_write_guard.base().await;
            (base.placement().zone().bloc().name().clone(), base.target().clone())
        };

        let should_start_combat =
            move_toward_target(&mut unit_write_guard, &unit_bloc, &target, &blocs_units_points, step).await;
        let self_position = unit_write_guard.position();
        let unit_id = unit_write_guard.id();
        drop(unit_write_guard);
        if should_start_combat {
            // If target is a base or trust and the unit is there, the combat is initiated accordingly
            let combat_params = match target {
                Target::Base { base, .. } => {
                    CombatParameters::Base(unit.clone(), base.clone(), base_destruction_threshold)
                }
                Target::Trust { trust, .. } => {
                    CombatParameters::Trust(unit.clone(), trust.clone(), trust_destruction_threshold)
                }
                Target::None => {
                    // Otherwise, it's a unit-only combat.
                    //
                    // Note: we need to account for the case where the unit has moved to a position where there are
                    // multiple units of an enemy bloc.

                    log::info!("Unit {} engages in combat with other units", unit_id);
                    let mut units_by_bloc = HashMap::from([(unit_bloc.clone(), vec![unit.clone()])]);

                    for other_unit in units.values() {
                        if Arc::ptr_eq(unit, other_unit) {
                            continue;
                        }

                        let other_unit_guard = other_unit.read().await;
                        // So, filter units by position,
                        if other_unit_guard.position() != self_position {
                            continue;
                        }

                        let other_unit_bloc = other_unit_guard.base().await.bloc().name().clone();
                        // and group them by bloc
                        units_by_bloc
                            .entry(other_unit_bloc)
                            .or_default()
                            .push(other_unit.clone());
                    }

                    // instantiate unit combat
                    CombatParameters::Units(units_by_bloc)
                }
            };

            let new_combat = Combat::new(combat_params).await;
            if let Some(existing_combat) = combats.get(&new_combat.position()) {
                let mut exististing_combat_guard = existing_combat.write().await;
                exististing_combat_guard.merge(new_combat);
            } else {
                combats.insert(new_combat.position(), Arc::new(RwLock::new(new_combat)));
            }
        }
    }
}

/// Core sync helper: given a unit's current position, its optional designated `target_point`, and
/// all unit snapshots, returns where the unit should move — or `None` when there is nothing to
/// move toward (no designated target and no enemies).
///
/// An enemy unit that is strictly closer than `target_point` always wins over the designated
/// target. When `target_point` is `None` the closest enemy unit is returned unconditionally.
pub(super) fn select_move_target(
    from: Point,
    unit_id: UnitId,
    unit_bloc: &BlocName,
    target_point: Option<Point>,
    all_units: &[(BlocName, UnitId, Point)],
) -> MoveTo {
    let enemies = || {
        all_units
            .iter()
            .filter(|(bloc, id, _)| bloc != unit_bloc && *id != unit_id)
    };

    match target_point {
        None => enemies()
            .min_by(|(_, _, a), (_, _, b)| {
                from.distance_to(a)
                    .partial_cmp(&from.distance_to(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(_, id, pos)| MoveTo::EnemyUnit(*id, *pos))
            .unwrap_or(MoveTo::None),
        Some(target) => {
            let target_dist = from.distance_to(&target);
            let closer_enemy = enemies()
                .filter(|(_, _, pos)| from.distance_to(pos) < target_dist)
                .min_by(|(_, _, a), (_, _, b)| {
                    from.distance_to(a)
                        .partial_cmp(&from.distance_to(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            match closer_enemy {
                Some((_, id, pos)) => MoveTo::EnemyUnit(*id, *pos),
                None => MoveTo::Designated(target),
            }
        }
    }
}
