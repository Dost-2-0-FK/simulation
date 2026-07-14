mod error;

use std::{collections::HashMap, fmt, sync::Arc, time::Duration};

use actix_web::cookie::Key;
use serde::Deserialize;
use serde_with::{DurationSecondsWithFrac, serde_as};
use tokio::{fs, net::TcpListener, sync::RwLock};

use self::error::{Error, Result};
use crate::{
    domain::{
        Bloc, BlocName, Chance, LootFactors, MilitaryBase, MilitaryUnit, Placement, PlacementId, Trust, Zone, ZoneName,
    },
    geometry::{Distance, Point, WorldBounds},
    services::credit_exchange_service::{Cost, CreditExchangeService, Share, VecResourceName},
};

const CONFIG_FILE_NAME: &str = "simulation.toml";

// TODO Placements must be part of config
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlConfig {
    server: ServerConfig,
    env: EnvConfig,
    bank_user_id: String,
    resources: VecResourceName,
    combat: CombatConfig,
    persistence: PersistenceConfig,
    costs: CostsConfig,
    world: WorldBounds,
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
    combat_tick_interval: Duration,
    blocs: Vec<Arc<RwLock<Bloc>>>,
    port: TcpListener,
    credit_exchange_service: CreditExchangeService,
    persistence: PersistenceConfig,
    movement_interval: Duration,
    movement_step: Distance,
    world_bounds: WorldBounds,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
    auth_cookie_key: Key,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvConfig {
    credit_exchange_url: url::Url,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerConfig {
    #[serde(default)]
    port: u16,
    auth_cookie_key: AuthCookieKey,
}

struct AuthCookieKey(Key);

impl fmt::Debug for AuthCookieKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AuthCookieKey(<redacted>)")
    }
}

impl<'de> Deserialize<'de> for AuthCookieKey {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let key = String::deserialize(deserializer)?;
        if key.len() != 64 {
            return Err(serde::de::Error::custom(
                "server.auth_cookie_key must contain exactly 64 bytes",
            ));
        }

        Ok(Self(Key::from(key.as_bytes())))
    }
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
struct CombatConfig {
    #[serde(rename = "combat_tick_interval_seconds")]
    #[serde_as(as = "DurationSecondsWithFrac<f64>")]
    combat_tick_interval: Duration,
    #[serde(rename = "movement_interval_seconds")]
    #[serde_as(as = "DurationSecondsWithFrac<f64>")]
    movement_interval: Duration,
    movement_step: Distance,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
    #[serde(default)]
    loot_factors: LootFactors,
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
    #[serde(default)]
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

    pub(crate) fn credit_exchange_service(&self) -> &CreditExchangeService {
        &self.credit_exchange_service
    }

    pub(crate) fn placements(&self) -> impl Iterator<Item = Arc<Placement>> + Clone + '_ {
        self.placements.iter().cloned()
    }

    pub(crate) fn zones(&self) -> impl Iterator<Item = Arc<Zone>> + '_ {
        self.zones.iter().cloned()
    }

    pub(crate) fn blocs(&self) -> impl Iterator<Item = Arc<RwLock<Bloc>>> + '_ {
        self.blocs.iter().cloned()
    }

    pub(crate) fn persistence(&self) -> &PersistenceConfig {
        &self.persistence
    }

    pub(crate) fn movement_interval(&self) -> Duration {
        self.movement_interval
    }

    pub(crate) fn combat_tick_interval(&self) -> Duration {
        self.combat_tick_interval
    }

    pub(crate) fn movement_step(&self) -> Distance {
        self.movement_step
    }

    pub(crate) fn world_bounds(&self) -> WorldBounds {
        self.world_bounds
    }

    pub(crate) fn base_destruction_threshold(&self) -> u32 {
        self.base_destruction_threshold
    }

    pub(crate) fn trust_destruction_threshold(&self) -> u32 {
        self.trust_destruction_threshold
    }

    pub(crate) fn auth_cookie_key(&self) -> Key {
        self.auth_cookie_key.clone()
    }

    async fn parse_from_str(config: &str) -> Result<Self> {
        let config = toml::from_str::<TomlConfig>(config).map_err(Error::Toml)?;
        config
            .world
            .validate()
            .map_err(|error| Error::ConfigValidation(error.to_string()))?;

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

        for resource_value in config.combat.loot_factors.resources() {
            if !config.resources.contains(resource_value.name()) {
                return Err(Error::ConfigValidation(format!(
                    "resource value {resource_value} is used in loot factors but is not listed in resources {resources}",
                    resources = &config.resources,
                )));
            }
        }

        let blocs = config
            .blocs
            .iter()
            .map(|bloc_config| {
                Arc::new(RwLock::new(Bloc::new(
                    bloc_config.name.clone(),
                    bloc_config.chance,
                    bloc_config.military_expense,
                )))
            })
            .collect::<Vec<_>>();

        let blocs_by_name = config
            .blocs
            .iter()
            .zip(blocs.iter())
            .map(|(bloc_config, bloc)| (bloc_config.name.clone(), bloc.clone()))
            .collect::<HashMap<_, _>>();

        let zones = config
            .zones
            .iter()
            .map(|zone_config| {
                let bloc = blocs_by_name.get(&zone_config.bloc).cloned().ok_or_else(|| {
                    Error::ConfigValidation(format!(
                        "zone {zone} references unknown bloc {bloc}",
                        zone = &zone_config.name,
                        bloc = &zone_config.bloc,
                    ))
                })?;

                let zone = Arc::new(Zone::new(zone_config.name.clone(), zone_config.bloc.clone(), bloc));

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
                    config.world.wrap(placement_config.position),
                ));

                Ok(placement)
            })
            .collect::<Result<Vec<_>>>()?;

        assert!(
            config.combat.movement_step > 0.0,
            "unit movement step must be greater than 0"
        );

        Ok(Self {
            placements,
            zones,
            combat_tick_interval: config.combat.combat_tick_interval,
            blocs,
            port: TcpListener::bind(format!("127.0.0.1:{}", config.server.port))
                .await
                .map_err(Error::Io)?,
            persistence: config.persistence,
            movement_interval: config.combat.movement_interval,
            movement_step: config.combat.movement_step,
            world_bounds: config.world,
            trust_destruction_threshold: config.combat.trust_destruction_threshold,
            base_destruction_threshold: config.combat.base_destruction_threshold,
            auth_cookie_key: config.server.auth_cookie_key.0,
            credit_exchange_service: CreditExchangeService::new(
                config.env.credit_exchange_url,
                config.bank_user_id,
                config.costs.unit,
                config.costs.base,
                config.costs.trust,
                config.resources,
                config.combat.loot_factors,
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use ordered_float::NotNan;

    use super::*;

    fn base_toml() -> &'static str {
        r#"
        resources = [
            "Lithium",
            "Iron",
        ]
        bank_user_id = "bank"

        [server]
        # No port: defaults to 0.
        auth_cookie_key = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

        [env]
        credit_exchange_url = "http://0.0.0.0:4534"

        [world]
        min_x = 0
        max_x = 30
        min_y = 0
        max_y = 30

        [combat]
        combat_tick_interval_seconds = 1.2
        movement_interval_seconds = 60
        movement_step = 1.0
        base_destruction_threshold = 4
        trust_destruction_threshold = 4

        [combat.loot_factors]
        money = 0.5
        resources = { lithium = 0.5 }

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
    async fn parse_normalizes_placements_to_world_bounds() {
        use crate::geometry::Positioned;

        let toml_str = base_toml().replace("x = 23.2, y = 29.1", "x = 53, y = 59");

        let config = Config::parse_from_str(&toml_str).await.expect("config can be parsed");
        let position = config.placements().next().expect("placement exists").position();

        assert_eq!(
            position,
            Point::new(NotNan::new(23.0).unwrap(), NotNan::new(29.0).unwrap())
        );
    }

    #[tokio::test]
    async fn parse_rejects_invalid_world_bounds() {
        let toml_str = base_toml().replace("min_x = 0", "min_x = 30");

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(error, "world min_x must be less than max_x");
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with invalid world bounds must fail"),
        }
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

    #[tokio::test]
    async fn parse_allows_omitted_loot_factors() {
        let toml_str = base_toml().replace(
            r#"
        [combat.loot_factors]
        money = 0.5
        resources = { lithium = 0.5 }
"#,
            "",
        );

        Config::parse_from_str(&toml_str)
            .await
            .expect("config can be parsed without loot factors");
    }

    #[tokio::test]
    async fn parse_rejects_unknown_loot_factor_resource() {
        let toml_str = base_toml().replace(
            "        resources = { lithium = 0.5 }",
            "        resources = { lithium = 0.5, copper = 0.5 }",
        );

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(
                    error,
                    "resource value ResourceValue(copper, 0.5) is used in loot factors but is not listed in resources [\"lithium\", \"iron\"]"
                );
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with unknown loot factor resource must fail"),
        }
    }
}
