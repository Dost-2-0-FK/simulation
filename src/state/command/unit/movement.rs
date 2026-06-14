use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    domain::{BlocName, MilitaryUnit, Target, UnitId},
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
fn move_toward_target_or_unit(
    unit: &mut MilitaryUnit,
    unit_bloc: &BlocName,
    target_point: Option<Point>,
    units: &[(BlocName, UnitId, Point)],
    step: Distance,
) {
    let from = unit.position();
    let move_to = match select_move_target(from, unit.id(), unit_bloc, target_point, units) {
        MoveTo::None => return,
        MoveTo::EnemyUnit(_, pos) | MoveTo::Designated(pos) => pos,
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
        blocs_units_points.push((bloc_name, *unit_id, unit.position()));
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
