use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    domain::{
        BaseId, Bloc, BlocKey, Combat, MilitaryBase, MilitaryUnit, ProductionUnit, ProductionUnitKey, Trust, TrustId,
        UnitId, Zone,
    },
    geometry::Point,
    persistence::MongoPersistence,
};

#[expect(
    clippy::too_many_arguments,
    reason = "persists each top-level simulation state collection"
)]
pub(crate) async fn persist_all(
    persistence: &MongoPersistence,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    production_units: &HashMap<ProductionUnitKey, Arc<RwLock<ProductionUnit>>>,
    blocs: &HashMap<BlocKey, Arc<RwLock<Bloc>>>,
    combats: &HashMap<Point, Arc<RwLock<Combat>>>,
    zones: impl Iterator<Item = Arc<Zone>>,
) {
    let mut unit_ids = Vec::with_capacity(units.len());
    for unit in units.values() {
        let unit = unit.read().await;
        unit_ids.push(unit.id().into());
        if let Err(e) = persistence.save_unit(&unit).await {
            log::error!("Error persisting unit: {e:#}");
        }
    }
    if let Err(e) = persistence.delete_units_except(unit_ids).await {
        log::error!("Error deleting stale units: {e:#}");
    }

    let mut base_ids = Vec::with_capacity(bases.len());
    for base in bases.values() {
        let base = base.read().await;
        base_ids.push(base.id().0.to_string());
        if let Err(e) = persistence.save_base(&base).await {
            log::error!("Error persisting base: {e:#}");
        }
    }
    if let Err(e) = persistence.delete_bases_except(base_ids).await {
        log::error!("Error deleting stale bases: {e:#}");
    }

    let mut trust_ids = Vec::with_capacity(trusts.len());
    for trust in trusts.values() {
        let trust = trust.read().await;
        trust_ids.push(trust.id().0.to_string());
        if let Err(e) = persistence.save_trust(&trust).await {
            log::error!("Error persisting trust: {e:#}");
        }
    }
    if let Err(e) = persistence.delete_trusts_except(trust_ids).await {
        log::error!("Error deleting stale trusts: {e:#}");
    }

    let mut production_unit_keys = Vec::with_capacity(production_units.len());
    for production_unit in production_units.values() {
        let production_unit = production_unit.read().await;
        production_unit_keys.push(production_unit.key().clone());
        if let Err(e) = persistence.save_production_unit(&production_unit).await {
            log::error!("Error persisting production unit: {e:#}");
        }
    }
    if let Err(e) = persistence.delete_production_units_except(production_unit_keys).await {
        log::error!("Error deleting stale production units: {e:#}");
    }

    for bloc in blocs.values() {
        if let Err(e) = persistence.save_bloc(&*bloc.read().await).await {
            log::error!("Error persisting bloc: {e:#}");
        }
    }

    let zones = zones.collect::<Vec<_>>();
    let mut zone_keys = Vec::with_capacity(zones.len());
    for zone in zones {
        zone_keys.push(zone.key().clone());
        if let Err(e) = persistence.save_zone(&zone).await {
            log::error!("Error persisting zone social-rule levels: {e:#}");
        }
    }
    if let Err(e) = persistence.delete_zones_except(zone_keys).await {
        log::error!("Error deleting stale zones: {e:#}");
    }

    let mut combat_ids = Vec::with_capacity(combats.len());
    for combat in combats.values() {
        let combat = combat.read().await;
        combat_ids.push(combat.id().into());
        if let Err(e) = persistence.save_combat(&combat).await {
            log::error!("Error persisting combat: {e:#}");
        }
    }
    if let Err(e) = persistence.delete_combats_except(combat_ids).await {
        log::error!("Error deleting stale combats: {e:#}");
    }

    log::info!("successfully persisted all collections.")
}
