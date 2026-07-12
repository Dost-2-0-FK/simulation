use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    domain::{BlocName, Combat, CombatParameters, CombatState, MilitaryUnit, Target, UnitId},
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
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    step: Distance,
) -> bool {
    let target_position = match target {
        Target::None => None,
        Target::Base { base, .. } => Some(base.read().await.position()),
        Target::Trust { trust, .. } => Some(trust.read().await.position()),
    };

    let from = unit.position();
    let to = match select_move_target(from, unit.id(), unit_bloc, target_position, units).await {
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
pub(crate) async fn move_units(
    units: &mut HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    combats: &mut HashMap<Point, Arc<RwLock<Combat>>>,
    step: Distance,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
) {
    let mut ended_combats = Vec::new();
    for (position, combat) in combats.iter() {
        if combat.read().await.state() == CombatState::Ended {
            ended_combats.push(*position);
        }
    }
    combats.retain(|position, _| !ended_combats.contains(position));

    for unit in units.values() {
        let mut unit_write_guard = unit.write().await;
        let (unit_bloc, target) = {
            let base = unit_write_guard.base().await;
            (base.bloc_name().clone(), base.target().clone())
        };

        let should_start_combat = move_toward_target(&mut unit_write_guard, &unit_bloc, &target, units, step).await;
        let self_position = unit_write_guard.position();
        let unit_id = unit_write_guard.id();
        drop(unit_write_guard);
        if should_start_combat {
            if let Some(existing_combat) = combats.get(&self_position) {
                if existing_combat.write().await.include_unit(unit.clone()).await {
                    log::debug!("Unit {} joins existing combat at position {self_position:?}", unit_id);
                }
                continue;
            }

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

                    log::debug!("Unit {} engages in new combat with other units", unit_id);
                    let mut units_by_bloc =
                        HashMap::from([(unit_bloc.clone(), HashMap::from([(unit_id, unit.clone())]))]);

                    for other_unit in units.values() {
                        if Arc::ptr_eq(unit, other_unit) {
                            continue;
                        }

                        let other_unit_guard = other_unit.read().await;
                        // So, filter units by position,
                        if other_unit_guard.position() != self_position {
                            continue;
                        }

                        let other_unit_bloc = other_unit_guard.base().await.bloc_name().clone();
                        let other_unit_id = other_unit_guard.id();
                        // and group them by bloc
                        units_by_bloc
                            .entry(other_unit_bloc)
                            .or_default()
                            .insert(other_unit_id, other_unit.clone());
                    }

                    // instantiate unit combat
                    CombatParameters::Units(units_by_bloc)
                }
            };

            let new_combat = Combat::new(combat_params).await;
            combats.insert(new_combat.position(), Arc::new(RwLock::new(new_combat)));
        }
    }
}

/// Core helper: given a unit's current position, its optional designated `target_point`, and
/// all live units, returns where the unit should move — or `None` when there is nothing to
/// move toward (no designated target and no enemies).
///
/// An enemy unit that is strictly closer than `target_point` always wins over the designated
/// target. When `target_point` is `None` the closest enemy unit is returned unconditionally.
pub(super) async fn select_move_target(
    from: Point,
    unit_id: UnitId,
    unit_bloc: &BlocName,
    target_point: Option<Point>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
) -> MoveTo {
    let mut closest_enemy: Option<(UnitId, Point, Distance)> = None;
    for (other_id, other_unit) in units {
        if *other_id == unit_id {
            continue;
        }

        let other_unit = other_unit.read().await;
        let other_unit_bloc = other_unit.base().await.bloc_name().clone();
        if other_unit_bloc == *unit_bloc {
            continue;
        }

        let position = other_unit.position();
        let distance = from.distance_to(&position);
        let is_closer = closest_enemy
            .as_ref()
            .map(|(_, _, closest_distance)| distance < *closest_distance)
            .unwrap_or(true);
        if is_closer {
            closest_enemy = Some((*other_id, position, distance));
        }
    }

    match target_point {
        None => closest_enemy
            .map(|(id, position, _)| MoveTo::EnemyUnit(id, position))
            .unwrap_or(MoveTo::None),
        Some(target) => {
            let target_dist = from.distance_to(&target);
            match closest_enemy {
                Some((id, position, distance)) if distance < target_dist => MoveTo::EnemyUnit(id, position),
                _ => MoveTo::Designated(target),
            }
        }
    }
}
