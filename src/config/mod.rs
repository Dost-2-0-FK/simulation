mod error;

use std::{sync::Arc, time::Duration};

use serde::Deserialize;
use serde_with::{DurationSecondsWithFrac, serde_as};
use tokio::{fs, net::TcpListener};

use self::error::{Error, Result};
use crate::{
    domain::{Bloc, BlocName, Chance, MilitaryBase, MilitaryUnit, Placement, PlacementId, Trust, Zone, ZoneName},
    geometry::Point,
    services::payment_service::Share,
    services::payment_service::{Cost, PaymentService, VecResourceName},
};

const CONFIG_FILE_NAME: &str = "simulation.toml";

// TODO Placements must be part of config
// TODO World bounds must be part of config

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlConfig {
    server: Option<ServerConfig>,
    env: EnvConfig,
    resources: VecResourceName,
    time: TimeConfig,
    persistence: PersistenceConfig,
    costs: CostsConfig,
    #[serde(rename = "bloc")]
    blocs: Vec<BlocConfig>,
    #[serde(rename = "zone")]
    zones: Vec<ZoneConfig>,
    #[serde(rename = "placement")]
    placements: Vec<PlacementConfig>,
}

#[expect(unused)]
pub(crate) struct Config {
    placements: Vec<Arc<Placement>>,
    zones: Vec<Arc<Zone>>,
    main_loop_tick: Duration,
    combat_loop_tick_factor: u8,
    blocs: Vec<Arc<Bloc>>,
    port: TcpListener,
    payment_service: PaymentService,
    persistence: PersistenceConfig,
    production_interval: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvConfig {
    credit_exchange_url: url::Url,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerConfig {
    #[serde(default)]
    port: u16,
}

#[serde_as]
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PersistenceConfig {
    uri: String,
    database: String,
    #[serde(rename = "interval_seconds")]
    #[serde_as(as = "DurationSecondsWithFrac<f64>")]
    interval: Duration,
}

impl PersistenceConfig {
    pub(crate) fn uri(&self) -> &str {
        &self.uri
    }

    pub(crate) fn database(&self) -> &str {
        &self.database
    }

    pub(crate) fn interval(&self) -> Duration {
        self.interval
    }
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeConfig {
    #[serde(rename = "main_loop_tick_seconds")]
    #[serde_as(as = "DurationSecondsWithFrac<f64>")]
    main_loop_tick: Duration,
    combat_loop_tick_factor: u8,
    #[serde(rename = "production_interval_seconds")]
    #[serde_as(as = "DurationSecondsWithFrac<f64>")]
    production_interval: Duration,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlacementConfig {
    pub(crate) id: PlacementId,
    pub(crate) zone: ZoneName,
    pub(crate) position: Point,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlocConfig {
    name: BlocName,
    chance: Chance,
    #[serde(default, rename = "militaryExpense")]
    military_expense: Share,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneConfig {
    name: ZoneName,
    bloc: BlocName,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CostsConfig {
    trust: Cost<Trust>,
    base: Cost<MilitaryBase>,
    unit: Cost<MilitaryUnit>,
}

impl Config {
    pub(crate) async fn parse() -> Result<Self> {
        let config = fs::read_to_string(CONFIG_FILE_NAME).await.map_err(Error::Io)?;
        Self::parse_from_str(&config).await
    }

    pub(crate) fn payment_service(&self) -> &PaymentService {
        &self.payment_service
    }

    pub(crate) fn placements(&self) -> impl Iterator<Item = Arc<Placement>> + Clone + '_ {
        self.placements.iter().cloned()
    }

    pub(crate) fn zones(&self) -> impl Iterator<Item = Arc<Zone>> + '_ {
        self.zones.iter().cloned()
    }

    pub(crate) fn blocs(&self) -> impl Iterator<Item = Arc<Bloc>> + '_ {
        self.blocs.iter().cloned()
    }

    pub(crate) fn persistence(&self) -> &PersistenceConfig {
        &self.persistence
    }

    pub(crate) fn production_interval(&self) -> Duration {
        self.production_interval
    }

    async fn parse_from_str(config: &str) -> Result<Self> {
        let config = toml::from_str::<TomlConfig>(config).map_err(Error::Toml)?;

        let resources_in_costs = config
            .costs
            .trust
            .resources()
            .chain(config.costs.base.resources())
            .chain(config.costs.unit.resources());

        for resource_value in resources_in_costs {
            if !config.resources.contains(resource_value.name()) {
                return Err(Error::ConfigValidation(format!(
                    "resource value {resource_value} is used in costs but is not listed in resources {resources}",
                    resources = &config.resources,
                )));
            }
        }

        let blocs = config
            .blocs
            .iter()
            .map(|bloc_config| {
                Arc::new(Bloc::new(
                    bloc_config.name.clone(),
                    bloc_config.chance,
                    bloc_config.military_expense,
                ))
            })
            .collect::<Vec<_>>();

        let zones = config
            .zones
            .iter()
            .map(|zone_config| {
                let bloc = blocs
                    .iter()
                    .find(|bloc| bloc.name() == &zone_config.bloc)
                    .cloned()
                    .ok_or_else(|| {
                        Error::ConfigValidation(format!(
                            "zone {zone} references unknown bloc {bloc}",
                            zone = &zone_config.name,
                            bloc = &zone_config.bloc,
                        ))
                    })?;

                let zone = Arc::new(Zone::new(zone_config.name.clone(), bloc));

                Ok(zone)
            })
            .collect::<Result<Vec<_>>>()?;

        let placements: Vec<_> = config
            .placements
            .iter()
            .map(|placement_config| {
                let zone = zones
                    .iter()
                    .find(|zone| zone.name() == &placement_config.zone)
                    .cloned()
                    .ok_or_else(|| {
                        Error::ConfigValidation(format!(
                            "placement references unknown zone {zone}",
                            zone = &placement_config.zone,
                        ))
                    })?;

                let placement = Arc::new(Placement::new(
                    placement_config.id.clone(),
                    zone,
                    placement_config.position,
                ));

                Ok(placement)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(Self {
            placements,
            zones,
            main_loop_tick: config.time.main_loop_tick,
            combat_loop_tick_factor: config.time.combat_loop_tick_factor,
            blocs,
            port: TcpListener::bind(format!("127.0.0.1:{}", config.server.unwrap_or_default().port))
                .await
                .map_err(Error::Io)?,
            persistence: config.persistence,
            production_interval: config.time.production_interval,
            payment_service: PaymentService::new(
                config.env.credit_exchange_url,
                config.costs.unit,
                config.costs.base,
                config.costs.trust,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_toml() -> &'static str {
        r#"
        resources = [
            "Lithium",
            "Iron",
        ]

        [server]
        # No port: defaults to 0.

        [env]
        credit_exchange_url = "http://0.0.0.0:4534"

        [time]
        main_loop_tick_seconds = 1.2
        combat_loop_tick_factor = 2
        production_interval_seconds = 3600

        [costs]
        base = { money = 1.5, resources = { lithium = 5.2, iron = 10.5 } }
        unit = { money = 1.5, resources = { lithium = 5.2, iron = 10.5 } }

        [costs.trust.resources]
        lithium = 1.5
        iron = 2.5

        [costs.trust]
        money = 1.2

        [persistence]
        uri = "mongodb://localhost:27017"
        database = "simulation"
        interval_seconds = 30

        [[bloc]]
        name = "bloc_1"
        chance = 12

        [[zone]]
        name = "zone_1"
        bloc = "bloc_1"

        [[placement]]
        id = "placement_1"
        zone = "zone_1"
        position = { x = 23.2, y = 29.1 }

        [[placement]]
        id = "placement_2"
        zone = "zone_1"
        position = { x = 23.2, y = 29.1 }
        "#
    }

    #[tokio::test]
    async fn parse() {
        Config::parse_from_str(base_toml()).await.expect("config can be parsed");
    }

    #[tokio::test]
    async fn parse_rejects_unknown_zone_bloc() {
        let toml_str = base_toml().replace("bloc = \"bloc_1\"", "bloc = \"bloc_2\"");

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(error, "zone zone_1 references unknown bloc bloc_2");
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with unknown zone bloc must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_unknown_placement_zone() {
        let toml_str = base_toml().replace("zone = \"zone_1\"", "zone = \"zone_name\"");

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(error, "placement references unknown zone zone_name");
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with unknown placement zone must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_unknown_cost_resource() {
        let toml_str = base_toml().replace("        iron = 2.5", "        iron = 2.5\n        copper = 3.5");

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(
                    error,
                    "resource value ResourceValue(copper, 3.5) is used in costs but is not listed in resources [\"lithium\", \"iron\"]"
                );
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with unknown cost resource must fail"),
        }
    }
}
