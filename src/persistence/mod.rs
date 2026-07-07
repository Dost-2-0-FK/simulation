use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow};
use futures_util::TryStreamExt;
use mongodb::{Client, Collection, bson::doc, options::ClientOptions};
use tokio::sync::RwLock;

use crate::{
    config::PersistenceConfig,
    domain::{BaseId, Bloc, Combat, MilitaryBase, MilitaryUnit, Placement, PlacementId, Trust, TrustId, UnitId},
    geometry::Point,
};

mod bases;
mod blocs;
mod combats;
mod trusts;
mod units;

use bases::PersistedBase;
use blocs::PersistedBloc;
use combats::PersistedCombat;
use trusts::PersistedTrust;
use units::PersistedUnit;

#[derive(Clone)]
pub(crate) struct MongoPersistence {
    bases: Collection<PersistedBase>,
    trusts: Collection<PersistedTrust>,
    units: Collection<PersistedUnit>,
    blocs: Collection<PersistedBloc>,
    combats: Collection<PersistedCombat>,
}

pub(crate) struct LoadedState {
    pub(crate) bases: HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    pub(crate) trusts: HashMap<TrustId, Arc<RwLock<Trust>>>,
    pub(crate) units: HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    pub(crate) blocs: Vec<Bloc>,
    pub(crate) combats: HashMap<Point, Arc<RwLock<Combat>>>,
}

impl MongoPersistence {
    pub(crate) async fn connect(config: &PersistenceConfig) -> Result<Self> {
        let mut options = ClientOptions::parse(config.uri())
            .await
            .with_context(|| format!("parsing MongoDB URI {}", config.uri()))?;
        options.server_selection_timeout = Some(Duration::from_secs(5));

        let client =
            Client::with_options(options).with_context(|| format!("creating MongoDB client for {}", config.uri()))?;

        client
            .database("admin")
            .run_command(doc! { "ping": 1 })
            .await
            .with_context(|| format!("connecting to MongoDB at {}", config.uri()))?;

        let database = client.database(config.database());

        Ok(Self {
            bases: database.collection("bases"),
            trusts: database.collection("trusts"),
            units: database.collection("units"),
            blocs: database.collection("blocs"),
            combats: database.collection("combats"),
        })
    }

    pub(crate) async fn load(&self, placements: impl Iterator<Item = Arc<Placement>> + Clone) -> Result<LoadedState> {
        // Phase 1: load raw persisted bases and create MilitaryBase instances with Target::None.
        let raw_bases: Vec<PersistedBase> = self
            .bases
            .find(doc! {})
            .await
            .context("loading bases from MongoDB")?
            .try_collect()
            .await
            .context("reading persisted bases")?;

        let bases: HashMap<BaseId, Arc<RwLock<MilitaryBase>>> = raw_bases
            .iter()
            .map(|pb| pb.as_base(placements.clone()))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|base| (base.id(), Arc::new(RwLock::new(base))))
            .collect();

        let trusts: HashMap<TrustId, Arc<RwLock<Trust>>> = self
            .trusts
            .find(doc! {})
            .await
            .context("loading trusts from MongoDB")?
            .map_ok(|trust| trust.into_trust(placements.clone()))
            .try_collect::<Vec<_>>()
            .await
            .context("reading persisted trusts")?
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|trust| (trust.id(), Arc::new(RwLock::new(trust))))
            .collect();

        // Phase 2: resolve base targets now that all bases and trusts are available.
        for pb in &raw_bases {
            if let Some(target) = pb
                .resolve_target(&bases, &trusts)
                .with_context(|| format!("resolving target for base {}", pb.id()))?
            {
                let base_id = pb.id().parse::<u64>().map(BaseId).context("parsing base id")?;
                if let Some(arc) = bases.get(&base_id) {
                    arc.write().await.set_target(target);
                }
            }
        }

        let units = self
            .units
            .find(doc! {})
            .await
            .context("loading units from MongoDB")?
            .map_ok(|unit| unit.into_unit(&bases))
            .try_collect::<Vec<_>>()
            .await
            .context("reading persisted units")?
            .into_iter()
            .collect::<Result<Vec<_>>>()?;
        let units: HashMap<UnitId, Arc<RwLock<MilitaryUnit>>> = units
            .into_iter()
            .map(|unit| (unit.id(), Arc::new(RwLock::new(unit))))
            .collect();

        let blocs = self
            .blocs
            .find(doc! {})
            .await
            .context("loading bloc overrides from MongoDB")?
            .map_ok(PersistedBloc::into_bloc)
            .try_collect()
            .await
            .context("reading persisted bloc overrides")?;

        let combats = self
            .combats
            .find(doc! {})
            .await
            .context("loading combats from MongoDB")?
            .map_ok(|combat| combat.into_combat(&units, &bases, &trusts))
            .try_collect::<Vec<_>>()
            .await
            .context("reading persisted combats")?
            .into_iter()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .map(|combat| (combat.position(), Arc::new(RwLock::new(combat))))
            .collect();

        Ok(LoadedState {
            bases,
            trusts,
            units,
            blocs,
            combats,
        })
    }

    pub(crate) async fn save_base(&self, base: &MilitaryBase) -> Result<()> {
        let persisted = PersistedBase::from_base(base);
        self.bases
            .replace_one(doc! { "_id": persisted.id() }, persisted)
            .upsert(true)
            .await
            .context("saving base to MongoDB")?;
        Ok(())
    }

    pub(crate) async fn save_trust(&self, trust: &Trust) -> Result<()> {
        let persisted = PersistedTrust::from_trust(trust);
        self.trusts
            .replace_one(doc! { "_id": persisted.id() }, persisted)
            .upsert(true)
            .await
            .context("saving trust to MongoDB")?;
        Ok(())
    }

    pub(crate) async fn save_unit(&self, unit: &MilitaryUnit) -> Result<()> {
        let persisted = PersistedUnit::from_unit(unit).await;
        self.units
            .replace_one(doc! { "_id": persisted.id() }, persisted)
            .upsert(true)
            .await
            .context("saving unit to MongoDB")?;
        Ok(())
    }

    pub(crate) async fn save_bloc(&self, bloc: &Bloc) -> Result<()> {
        let persisted = PersistedBloc::from_bloc(bloc);
        self.blocs
            .replace_one(doc! { "_id": persisted.id() }, persisted)
            .upsert(true)
            .await
            .context("saving bloc override to MongoDB")?;
        Ok(())
    }

    pub(crate) async fn save_combat(&self, combat: &Combat) -> Result<()> {
        let persisted = PersistedCombat::from_combat(combat).await;
        self.combats
            .replace_one(doc! { "_id": persisted.id() }, persisted)
            .upsert(true)
            .await
            .context("saving combat to MongoDB")?;
        Ok(())
    }

    pub(crate) async fn delete_units_except(&self, ids: Vec<mongodb::bson::Uuid>) -> Result<()> {
        self.units
            .delete_many(doc! { "_id": { "$nin": ids } })
            .await
            .context("deleting stale units from MongoDB")?;
        Ok(())
    }

    pub(crate) async fn delete_bases_except(&self, ids: Vec<String>) -> Result<()> {
        self.bases
            .delete_many(doc! { "_id": { "$nin": ids } })
            .await
            .context("deleting stale bases from MongoDB")?;
        Ok(())
    }

    pub(crate) async fn delete_trusts_except(&self, ids: Vec<String>) -> Result<()> {
        self.trusts
            .delete_many(doc! { "_id": { "$nin": ids } })
            .await
            .context("deleting stale trusts from MongoDB")?;
        Ok(())
    }

    pub(crate) async fn delete_combats_except(&self, ids: Vec<mongodb::bson::Uuid>) -> Result<()> {
        self.combats
            .delete_many(doc! { "_id": { "$nin": ids } })
            .await
            .context("deleting stale combats from MongoDB")?;
        Ok(())
    }
}

fn placement_by_id(mut placements: impl Iterator<Item = Arc<Placement>>, id: &PlacementId) -> Result<Arc<Placement>> {
    placements
        .find(|placement| placement.id() == id)
        .ok_or_else(|| anyhow!("persisted placement {id} is not present in config"))
}

trait FromPersistedId {
    fn from_u64(id: u64) -> Self;
}

impl FromPersistedId for BaseId {
    fn from_u64(id: u64) -> Self {
        Self(id)
    }
}

impl FromPersistedId for TrustId {
    fn from_u64(id: u64) -> Self {
        Self(id)
    }
}

fn parse_id<T: FromPersistedId>(id: &str, entity: &str) -> Result<T> {
    id.parse::<u64>()
        .map(T::from_u64)
        .with_context(|| format!("parsing persisted {entity} id {id}"))
}
