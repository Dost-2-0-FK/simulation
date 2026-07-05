use std::{collections::HashMap, sync::Arc};

use tokio::sync::RwLock;

use crate::{
    domain::{BaseId, Bloc, BlocName, Combat, MilitaryBase, MilitaryUnit, Trust, TrustId, UnitId},
    geometry::Point,
    persistence::MongoPersistence,
};

pub(crate) async fn persist_all(
    persistence: &MongoPersistence,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    blocs: &HashMap<BlocName, Arc<RwLock<Bloc>>>,
    combats: &HashMap<Point, Arc<RwLock<Combat>>>,
) {
    for unit in units.values() {
        if let Err(e) = persistence.save_unit(&*unit.read().await).await {
            log::error!("Error persisting unit: {e:#}");
        }
    }
    for base in bases.values() {
        if let Err(e) = persistence.save_base(&*base.read().await).await {
            log::error!("Error persisting base: {e:#}");
        }
    }
    for trust in trusts.values() {
        if let Err(e) = persistence.save_trust(&*trust.read().await).await {
            log::error!("Error persisting trust: {e:#}");
        }
    }
    for bloc in blocs.values() {
        if let Err(e) = persistence.save_bloc(&*bloc.read().await).await {
            log::error!("Error persisting bloc: {e:#}");
        }
    }
    for combat in combats.values() {
        if let Err(e) = persistence.save_combat(&*combat.read().await).await {
            log::error!("Error persisting combat: {e:#}");
        }
    }
    log::info!("successfully persisted all collections.")
}
