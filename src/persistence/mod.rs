use std::{sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow};
use futures_util::TryStreamExt;
use mongodb::{Client, Collection, bson::doc, options::ClientOptions};

use crate::{
    config::PersistenceConfig,
    domain::{BaseId, Bloc, MilitaryBase, MilitaryUnit, Placement, PlacementId, Trust, TrustId},
};

mod bases;
mod blocs;
mod trusts;
mod units;

use bases::PersistedBase;
use blocs::PersistedBloc;
use trusts::PersistedTrust;
use units::PersistedUnit;

#[derive(Clone)]
pub(crate) struct MongoPersistence {
    bases: Collection<PersistedBase>,
    trusts: Collection<PersistedTrust>,
    units: Collection<PersistedUnit>,
    blocs: Collection<PersistedBloc>,
}

pub(crate) struct LoadedState {
    pub(crate) bases: Vec<MilitaryBase>,
    pub(crate) trusts: Vec<Trust>,
    pub(crate) units: Vec<MilitaryUnit>,
    pub(crate) blocs: Vec<Bloc>,
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
        })
    }

    pub(crate) async fn load(&self, placements: impl Iterator<Item = Arc<Placement>> + Clone) -> Result<LoadedState> {
        let bases = self
            .bases
            .find(doc! {})
            .await
            .context("loading bases from MongoDB")?
            .map_ok(|base| base.into_base(placements.clone()))
            .try_collect::<Vec<_>>()
            .await
            .context("reading persisted bases")?
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        let trusts = self
            .trusts
            .find(doc! {})
            .await
            .context("loading trusts from MongoDB")?
            .map_ok(|trust| trust.into_trust(placements.clone()))
            .try_collect::<Vec<_>>()
            .await
            .context("reading persisted trusts")?
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        let units = self
            .units
            .find(doc! {})
            .await
            .context("loading units from MongoDB")?
            .map_ok(PersistedUnit::into_unit)
            .try_collect::<Vec<_>>()
            .await
            .context("reading persisted units")?
            .into_iter()
            .collect::<Result<Vec<_>>>()?;

        let blocs = self
            .blocs
            .find(doc! {})
            .await
            .context("loading bloc overrides from MongoDB")?
            .map_ok(PersistedBloc::into_bloc)
            .try_collect()
            .await
            .context("reading persisted bloc overrides")?;

        Ok(LoadedState {
            bases,
            trusts,
            units,
            blocs,
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
        self.units
            .insert_one(PersistedUnit::from_unit(unit))
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
