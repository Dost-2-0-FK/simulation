mod base;
mod combat;
mod placement;
mod politics;
mod trust;
mod unit;

pub(crate) use base::{BaseId, MilitaryBase, Target};
pub(crate) use combat::{
    Combat, CombatId, CombatParameters, CombatState, CombatStructureParameters, CombatStructureSnapshot,
};
pub(crate) use placement::{Placement, PlacementId};
pub(crate) use politics::{Bloc, BlocName, Chance, Zone, ZoneName};
pub(crate) use trust::{Trust, TrustId};
pub(crate) use unit::{MilitaryUnit, UnitId, UnitState};
