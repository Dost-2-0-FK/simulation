mod base;
mod combat;
mod names;
mod placement;
mod politics;
mod trust;
mod unit;

pub(crate) use base::{BaseId, MilitaryBase, Target};
pub(crate) use combat::{
    Combat, CombatEvent, CombatId, CombatParameters, CombatState, CombatStructureParameters, CombatStructureSnapshot,
    LootTransfer, UnitKilled,
    loot::{Loot, LootFactors},
};
pub(crate) use placement::{Placement, PlacementId};
pub(crate) use names::NameMappings;
pub(crate) use politics::{
    Bloc, BlocKey, BlocName, Chance, CharacterKey, CharacterName, Zone, ZoneKey, ZoneName,
};
pub(crate) use trust::{Trust, TrustId};
pub(crate) use unit::{AttackOutcome, MilitaryUnit, UnitId, UnitState};
