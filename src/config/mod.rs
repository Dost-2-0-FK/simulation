mod error;

use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use actix_web::cookie::Key;
use serde::Deserialize;
use serde_with::{DurationSecondsWithFrac, serde_as};
use tokio::{fs, sync::RwLock};

use self::error::{Error, Result};
use crate::{
    domain::{
        BaseId, Bloc, BlocKey, BlocName, Chance, CharacterKey, CharacterName, LootFactors, MilitaryBase, MilitaryUnit,
        NameMappings, Placement, PlacementId, Trust, TrustId, Zone, ZoneKey, ZoneName,
    },
    geometry::{Distance, Point, WorldBounds},
    handlers::bases::Financing,
    services::{
        auth_service::AuthService,
        credit_exchange_service::{
            Cost, CreditExchangeService, Money, MoneyPerResource, ResourceName, Resources, Share, VecResourceName,
        },
    },
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
    trust_production: TrustProductionConfig,
    world: WorldBounds,
    #[serde(rename = "bloc")]
    blocs: Vec<BlocConfig>,
    #[serde(rename = "zone")]
    zones: Vec<ZoneConfig>,
    #[serde(rename = "character")]
    characters: Vec<CharacterConfig>,
    #[serde(rename = "placement")]
    placements: Vec<PlacementConfig>,
    #[serde(default, rename = "base")]
    bases: Vec<BaseConfig>,
    #[serde(default, rename = "trust")]
    trusts: Vec<TrustConfig>,
}

pub(crate) struct Config {
    placements: Vec<Arc<Placement>>,
    zones: Vec<Arc<Zone>>,
    combat_tick_interval: Duration,
    blocs: Vec<Arc<RwLock<Bloc>>>,
    server_address: SocketAddr,
    credit_exchange_service: CreditExchangeService,
    persistence: PersistenceConfig,
    movement_interval: Duration,
    movement_step: Distance,
    world_bounds: WorldBounds,
    trust_production_resources: Resources,
    trust_money_per_resource: MoneyPerResource,
    trust_inhibition_radius: Distance,
    trust_inhibition_factor_close_units: Share,
    trust_inhibition_factor_combat: Share,
    base_destruction_threshold: u32,
    trust_destruction_threshold: u32,
    auth_cookie_key: Key,
    auth_service: AuthService,
    name_mappings: Arc<NameMappings>,
    base_seeds: Vec<BaseConfig>,
    trust_seeds: Vec<TrustConfig>,
}

pub(crate) struct SeededStructures {
    pub(crate) bases: HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    pub(crate) trusts: HashMap<TrustId, Arc<RwLock<Trust>>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvConfig {
    auth_service_url: url::Url,
    credit_exchange_url: url::Url,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServerConfig {
    #[serde(default = "localhost")]
    bind_address: IpAddr,
    #[serde(default)]
    port: u16,
    auth_cookie_key: AuthCookieKey,
}

fn localhost() -> IpAddr {
    IpAddr::V4(Ipv4Addr::LOCALHOST)
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
    pub(crate) zone: ZoneKey,
    pub(crate) position: Point,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BaseConfig {
    placement_id: PlacementId,
    payment: Vec<Financing>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustConfig {
    placement_id: PlacementId,
    resource: ResourceName,
    payment: Vec<Financing>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustProductionConfig {
    money_per_resource: MoneyPerResource,
    resources: Resources,
    inhibition_radius: Distance,
    inhibition_factor_close_units: Share,
    inhibition_factor_combat: Share,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlocConfig {
    key: BlocKey,
    name: BlocName,
    chance: Chance,
    #[serde(default)]
    military_expense: Share,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ZoneConfig {
    key: ZoneKey,
    name: ZoneName,
    bloc: BlocKey,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CharacterConfig {
    key: CharacterKey,
    name: CharacterName,
}

fn validate_unique<'a, T>(
    kind: &str,
    values: impl Iterator<Item = &'a T>,
) -> Result<()>
where
    T: Eq + Hash + fmt::Display + 'a,
{
    let mut seen = HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(Error::ConfigValidation(format!("duplicate {kind} {value}")));
        }
    }
    Ok(())
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

    pub(crate) fn resources(&self) -> &[ResourceName] {
        self.credit_exchange_service.resources()
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

    pub(crate) fn trust_resource_production(&self, resource: &ResourceName) -> Option<f32> {
        self.trust_production_resources.get(resource)
    }

    pub(crate) fn trust_base_income(&self, resource: &ResourceName) -> Option<Money> {
        self.trust_money_per_resource.get(resource)
    }

    pub(crate) fn trust_inhibition_radius(&self) -> Distance {
        self.trust_inhibition_radius
    }

    pub(crate) fn trust_inhibition_factor_close_units(&self) -> Share {
        self.trust_inhibition_factor_close_units
    }

    pub(crate) fn trust_inhibition_factor_combat(&self) -> Share {
        self.trust_inhibition_factor_combat
    }

    pub(crate) fn auth_cookie_key(&self) -> Key {
        self.auth_cookie_key.clone()
    }

    pub(crate) fn auth_service(&self) -> &AuthService {
        &self.auth_service
    }

    pub(crate) fn name_mappings(&self) -> Arc<NameMappings> {
        self.name_mappings.clone()
    }

    pub(crate) fn server_address(&self) -> SocketAddr {
        self.server_address
    }

    pub(crate) fn seeded_structures(&self) -> SeededStructures {
        let bases = self
            .base_seeds
            .iter()
            .map(|seed| {
                let placement = self
                    .placements()
                    .find(|placement| placement.id() == &seed.placement_id)
                    .expect("seed placement references were validated while parsing config");
                let base = MilitaryBase::new_prepaid(
                    seed.payment.clone(),
                    &self.credit_exchange_service.military_base,
                    self.credit_exchange_service.loot_factors(),
                    placement,
                );
                (base.id(), Arc::new(RwLock::new(base)))
            })
            .collect();

        let trusts = self
            .trust_seeds
            .iter()
            .map(|seed| {
                let placement = self
                    .placements()
                    .find(|placement| placement.id() == &seed.placement_id)
                    .expect("seed placement references were validated while parsing config");
                let resource_amount = self
                    .trust_resource_production(&seed.resource)
                    .expect("seed trust resources were validated while parsing config");
                let base_income = self
                    .trust_base_income(&seed.resource)
                    .expect("seed trust money was validated while parsing config");
                let trust = Trust::new_prepaid(
                    seed.payment.clone(),
                    &self.credit_exchange_service.trust,
                    self.credit_exchange_service.loot_factors(),
                    placement,
                    seed.resource.clone(),
                    resource_amount,
                    base_income,
                );
                (trust.id(), Arc::new(RwLock::new(trust)))
            })
            .collect();

        SeededStructures { bases, trusts }
    }

    async fn parse_from_str(config: &str) -> Result<Self> {
        let config = toml::from_str::<TomlConfig>(config).map_err(Error::Toml)?;

        validate_unique(
            "bloc key",
            config.blocs.iter().map(|bloc| &bloc.key),
        )?;
        validate_unique(
            "bloc name",
            config.blocs.iter().map(|bloc| &bloc.name),
        )?;
        validate_unique(
            "zone key",
            config.zones.iter().map(|zone| &zone.key),
        )?;
        validate_unique(
            "zone name",
            config.zones.iter().map(|zone| &zone.name),
        )?;
        validate_unique(
            "character key",
            config.characters.iter().map(|character| &character.key),
        )?;
        validate_unique(
            "character name",
            config.characters.iter().map(|character| &character.name),
        )?;

        let name_mappings = Arc::new(NameMappings::new(
            config
                .blocs
                .iter()
                .map(|bloc| (bloc.key.clone(), bloc.name.clone()))
                .collect(),
            config
                .zones
                .iter()
                .map(|zone| (zone.key.clone(), zone.name.clone()))
                .collect(),
            config
                .characters
                .iter()
                .map(|character| (character.key.clone(), character.name.clone()))
                .collect(),
        ));

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

        for resource_value in &config.trust_production.resources {
            if !config.resources.contains(resource_value.name()) {
                return Err(Error::ConfigValidation(format!(
                    "resource value {resource_value} is used in trust production but is not listed in resources {resources}",
                    resources = &config.resources,
                )));
            }
            if config
                .trust_production
                .money_per_resource
                .get(resource_value.name())
                .is_none()
            {
                return Err(Error::ConfigValidation(format!(
                    "resource {resource} has configured trust production but no configured money production",
                    resource = resource_value.name(),
                )));
            }
        }

        for money_value in config.trust_production.money_per_resource.values() {
            if !config.resources.contains(money_value.name()) {
                return Err(Error::ConfigValidation(format!(
                    "resource {resource} has configured trust money production but is not listed in resources {resources}",
                    resource = money_value.name(),
                    resources = &config.resources,
                )));
            }
        }

        if config.trust_production.inhibition_factor_combat.value()
            > config.trust_production.inhibition_factor_close_units.value()
        {
            return Err(Error::ConfigValidation(
                "trust production inhibition_factor_combat must be less than or equal to inhibition_factor_close_units"
                    .to_string(),
            ));
        }

        let blocs = config
            .blocs
            .iter()
            .map(|bloc_config| {
                Arc::new(RwLock::new(Bloc::new(
                    bloc_config.key.clone(),
                    bloc_config.name.clone(),
                    bloc_config.chance,
                    bloc_config.military_expense,
                )))
            })
            .collect::<Vec<_>>();

        let blocs_by_key = config
            .blocs
            .iter()
            .zip(blocs.iter())
            .map(|(bloc_config, bloc)| (bloc_config.key.clone(), bloc.clone()))
            .collect::<HashMap<_, _>>();

        let zones = config
            .zones
            .iter()
            .map(|zone_config| {
                let bloc = blocs_by_key.get(&zone_config.bloc).cloned().ok_or_else(|| {
                    Error::ConfigValidation(format!(
                        "zone {zone} references unknown bloc {bloc}",
                        zone = &zone_config.name,
                        bloc = &zone_config.bloc,
                    ))
                })?;

                let bloc_name = name_mappings
                    .bloc_name(&zone_config.bloc)
                    .expect("bloc reference was resolved above")
                    .clone();
                let zone = Arc::new(Zone::new(
                    zone_config.key.clone(),
                    zone_config.name.clone(),
                    zone_config.bloc.clone(),
                    bloc_name,
                    bloc,
                ));

                Ok(zone)
            })
            .collect::<Result<Vec<_>>>()?;

        let placements: Vec<_> = config
            .placements
            .iter()
            .map(|placement_config| {
                if !config.world.contains(placement_config.position) {
                    return Err(Error::ConfigValidation(format!(
                        "placement {id} at {position:?} is outside world bounds",
                        id = &placement_config.id,
                        position = placement_config.position,
                    )));
                }

                let zone = zones
                    .iter()
                    .find(|zone| zone.key() == &placement_config.zone)
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

        let mut occupied_seed_placements = HashMap::new();
        for (kind, placement_id, payment) in config
            .bases
            .iter()
            .map(|seed| ("base", &seed.placement_id, &seed.payment))
            .chain(
                config
                    .trusts
                    .iter()
                    .map(|seed| ("trust", &seed.placement_id, &seed.payment)),
            )
        {
            if !placements.iter().any(|placement| placement.id() == placement_id) {
                return Err(Error::ConfigValidation(format!(
                    "seeded {kind} references unknown placement {placement_id}"
                )));
            }
            if let Some(previous_kind) = occupied_seed_placements.insert(placement_id.clone(), kind) {
                return Err(Error::ConfigValidation(format!(
                    "seeded {kind} and {previous_kind} both use placement {placement_id}"
                )));
            }

            let financier_share = payment.iter().map(|financing| financing.share.value()).sum::<f32>();
            if financier_share > 1.0 {
                return Err(Error::ConfigValidation(format!(
                    "seeded {kind} on placement {placement_id} has financier shares summing to {financier_share}, expected at most 1"
                )));
            }

            for financing in payment {
                if name_mappings.character_name(&financing.financier).is_none() {
                    return Err(Error::ConfigValidation(format!(
                        "seeded {kind} on placement {placement_id} references unknown character {}",
                        financing.financier,
                    )));
                }
            }
        }

        for trust in &config.trusts {
            if config.trust_production.resources.get(&trust.resource).is_none() {
                return Err(Error::ConfigValidation(format!(
                    "seeded trust on placement {placement_id} uses resource {resource} without configured trust production",
                    placement_id = &trust.placement_id,
                    resource = &trust.resource,
                )));
            }
        }

        assert!(
            config.combat.movement_step > 0.0,
            "unit movement step must be greater than 0"
        );

        let auth_service = AuthService::new(config.env.auth_service_url.clone(), name_mappings.clone());

        Ok(Self {
            placements,
            zones,
            combat_tick_interval: config.combat.combat_tick_interval,
            blocs,
            server_address: SocketAddr::new(config.server.bind_address, config.server.port),
            persistence: config.persistence,
            movement_interval: config.combat.movement_interval,
            movement_step: config.combat.movement_step,
            world_bounds: config.world,
            trust_production_resources: config.trust_production.resources,
            trust_money_per_resource: config.trust_production.money_per_resource,
            trust_inhibition_radius: config.trust_production.inhibition_radius,
            trust_inhibition_factor_close_units: config.trust_production.inhibition_factor_close_units,
            trust_inhibition_factor_combat: config.trust_production.inhibition_factor_combat,
            trust_destruction_threshold: config.combat.trust_destruction_threshold,
            base_destruction_threshold: config.combat.base_destruction_threshold,
            auth_cookie_key: config.server.auth_cookie_key.0,
            auth_service,
            name_mappings,
            base_seeds: config.bases,
            trust_seeds: config.trusts,
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
    use std::collections::HashSet;

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
        auth_service_url = "http://0.0.0.0:4535"
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

        [trust_production]
        money_per_resource = { lithium = 2.0, iron = 2.5 }
        inhibition_radius = 1.0
        inhibition_factor_close_units = 0.75
        inhibition_factor_combat = 0.25
        resources = { lithium = 3.5, iron = 4.5 }

        [persistence]
        uri = "mongodb://localhost:27017"
        database = "simulation"
        interval_seconds = 30

        [[bloc]]
        key = "bloc_1"
        name = "bloc_1"
        chance = 12

        [[zone]]
        key = "zone_1"
        name = "zone_1"
        bloc = "bloc_1"

        [[character]]
        key = "base-financier"
        name = "Base Financier"

        [[character]]
        key = "trust-financier"
        name = "Trust Financier"

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
        let config = Config::parse_from_str(base_toml()).await.expect("config can be parsed");

        assert_eq!(config.server_address(), "127.0.0.1:0".parse().unwrap());
    }

    #[tokio::test]
    async fn parse_server_address() {
        let toml_str = base_toml().replace(
            "        # No port: defaults to 0.",
            "        bind_address = \"0.0.0.0\"\n        port = 8080",
        );

        let config = Config::parse_from_str(&toml_str).await.expect("config can be parsed");

        assert_eq!(config.server_address(), "0.0.0.0:8080".parse().unwrap());
    }

    #[tokio::test]
    async fn parse_rejects_placement_outside_world_bounds() {
        let toml_str = base_toml().replace("x = 23.2, y = 29.1", "x = 53, y = 59");

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(
                    error,
                    "placement placement_1 at Point { x: 53.0, y: 59.0 } is outside world bounds"
                );
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with an out-of-bounds placement must fail"),
        }
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
    async fn parse_rejects_duplicate_keys_and_names() {
        let cases = [
            (
                r#"
                [[bloc]]
                key = "bloc_1"
                name = "another bloc"
                chance = 12
                "#,
                "duplicate bloc key bloc_1",
            ),
            (
                r#"
                [[bloc]]
                key = "another-bloc"
                name = "bloc_1"
                chance = 12
                "#,
                "duplicate bloc name bloc_1",
            ),
            (
                r#"
                [[zone]]
                key = "zone_1"
                name = "another zone"
                bloc = "bloc_1"
                "#,
                "duplicate zone key zone_1",
            ),
            (
                r#"
                [[zone]]
                key = "another-zone"
                name = "zone_1"
                bloc = "bloc_1"
                "#,
                "duplicate zone name zone_1",
            ),
            (
                r#"
                [[character]]
                key = "base-financier"
                name = "Another Character"
                "#,
                "duplicate character key base-financier",
            ),
            (
                r#"
                [[character]]
                key = "another-character"
                name = "Base Financier"
                "#,
                "duplicate character name Base Financier",
            ),
        ];

        for (duplicate, expected) in cases {
            let config = format!("{}\n{duplicate}", base_toml());
            match Config::parse_from_str(&config).await {
                Err(Error::ConfigValidation(error)) => assert_eq!(error, expected),
                Err(error) => panic!("unexpected error: {error}"),
                Ok(_) => panic!("config with {expected} must fail"),
            }
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

    #[tokio::test]
    async fn parse_rejects_unknown_trust_production_resource() {
        let toml_str = base_toml().replace(
            r#"
        [trust_production]
        money_per_resource = { lithium = 2.0, iron = 2.5 }
        inhibition_radius = 1.0
        inhibition_factor_close_units = 0.75
        inhibition_factor_combat = 0.25
        resources = { lithium = 3.5, iron = 4.5 }
"#,
            r#"
        [trust_production]
        money_per_resource = { lithium = 2.0, iron = 2.5 }
        inhibition_radius = 1.0
        inhibition_factor_close_units = 0.75
        inhibition_factor_combat = 0.25
        resources = { lithium = 3.5, iron = 4.5, copper = 4.5 }
"#,
        );

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(
                    error,
                    "resource value ResourceValue(copper, 4.5) is used in trust production but is not listed in resources [\"lithium\", \"iron\"]"
                );
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with unknown trust production resource must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_trust_resource_without_money_production() {
        let toml_str = base_toml().replace(
            "        money_per_resource = { lithium = 2.0, iron = 2.5 }",
            "        money_per_resource = { lithium = 2.0 }",
        );

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => assert_eq!(
                error,
                "resource iron has configured trust production but no configured money production"
            ),
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("trust resource without money production must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_unknown_trust_money_resource() {
        let toml_str = base_toml().replace(
            "        money_per_resource = { lithium = 2.0, iron = 2.5 }",
            "        money_per_resource = { lithium = 2.0, iron = 2.5, copper = 3.0 }",
        );

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => assert_eq!(
                error,
                "resource copper has configured trust money production but is not listed in resources [\"lithium\", \"iron\"]"
            ),
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("trust money production for an unknown resource must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_combat_inhibition_factor_above_close_units_factor() {
        let toml_str = base_toml().replace(
            "        inhibition_factor_combat = 0.25",
            "        inhibition_factor_combat = 0.8",
        );

        match Config::parse_from_str(&toml_str).await {
            Err(Error::ConfigValidation(error)) => assert_eq!(
                error,
                "trust production inhibition_factor_combat must be less than or equal to inhibition_factor_close_units"
            ),
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("config with combat inhibition above close-unit inhibition must fail"),
        }
    }

    fn with_structure_seeds() -> String {
        format!(
            r#"{}

        [[base]]
        placement_id = "placement_1"
        payment = [{{ financierId = "base-financier", share = 0.25 }}]

        [[trust]]
        placement_id = "placement_2"
        resource = "Lithium"
        payment = [{{ financierId = "trust-financier", share = 0.4 }}]
        "#,
            base_toml()
        )
    }

    #[tokio::test]
    async fn parse_builds_prepaid_structure_seeds() {
        let config = Config::parse_from_str(&with_structure_seeds())
            .await
            .expect("config with structure seeds can be parsed");
        let seeded = config.seeded_structures();

        assert_eq!(seeded.bases.len(), 1);
        assert_eq!(seeded.trusts.len(), 1);

        let base = seeded.bases.values().next().unwrap().read().await;
        assert_eq!(base.placement_id().to_string(), "placement_1");
        assert_eq!(base.financiers().len(), 1);

        let trust = seeded.trusts.values().next().unwrap().read().await;
        assert_eq!(trust.placement_id().to_string(), "placement_2");
        assert_eq!(trust.base_income().value(), 2.0);
        assert_eq!(
            trust.producing_base_value().get(&ResourceName::new("lithium".into())),
            Some(3.5)
        );
    }

    #[tokio::test]
    async fn parse_checked_in_config() {
        let config = Config::parse_from_str(include_str!("../../simulation.toml"))
            .await
            .expect("checked-in config can be parsed");
        let seeded = config.seeded_structures();

        assert_eq!(seeded.bases.len(), 11);
        assert_eq!(seeded.trusts.len(), 35);
    }

    #[test]
    fn credit_exchange_seed_matches_configured_receivers() {
        let config = toml::from_str::<TomlConfig>(include_str!("../../simulation.toml"))
            .expect("checked-in config TOML can be deserialized");
        let seed = serde_json::from_str::<serde_json::Value>(include_str!(
            "../../docker/credit-exchanger/credit-exchanger.json"
        ))
        .expect("credit-exchange seed JSON can be deserialized");

        let seed_ids = |key| {
            seed[key]
                .as_array()
                .expect("seed collection is an array")
                .iter()
                .map(|entry| entry["id"].as_str().expect("seed user has a string ID"))
                .collect::<HashSet<_>>()
        };

        let expected_blocs = config
            .blocs
            .iter()
            .map(|bloc| bloc.key.to_string())
            .collect::<HashSet<_>>();
        let expected_zones = config
            .zones
            .iter()
            .map(|zone| zone.key.to_string())
            .collect::<HashSet<_>>();
        let expected_individuals = config
            .bases
            .iter()
            .flat_map(|base| &base.payment)
            .chain(config.trusts.iter().flat_map(|trust| &trust.payment))
            .map(|financing| financing.financier.as_str())
            .collect::<HashSet<_>>();
        let seeded_resource_users = seed_ids("blocs")
            .into_iter()
            .chain(seed_ids("zones"))
            .collect::<HashSet<_>>();

        assert!(
            seed_ids("blocs").is_superset(&expected_blocs.iter().map(String::as_str).collect()),
            "credit-exchange seed is missing a configured bloc"
        );
        assert!(
            seed_ids("zones").is_superset(&expected_zones.iter().map(String::as_str).collect()),
            "credit-exchange seed is missing a configured zone"
        );
        assert!(
            seed_ids("individuals").is_superset(&expected_individuals),
            "credit-exchange seed is missing a configured financier"
        );
        assert!(
            seeded_resource_users.contains(config.bank_user_id.as_str()),
            "credit-exchange seed bank user must be a bloc or zone"
        );
    }

    #[tokio::test]
    async fn parse_rejects_seed_with_unknown_placement() {
        let toml = with_structure_seeds().replace("placement_id = \"placement_1\"", "placement_id = \"missing\"");

        match Config::parse_from_str(&toml).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(error, "seeded base references unknown placement missing");
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("seed with unknown placement must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_seeds_on_same_placement() {
        let toml = with_structure_seeds().replace("placement_id = \"placement_2\"", "placement_id = \"placement_1\"");

        match Config::parse_from_str(&toml).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(error, "seeded trust and base both use placement placement_1");
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("two seeds on the same placement must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_seed_with_excessive_financier_shares() {
        let toml = with_structure_seeds().replace(
            "payment = [{ financierId = \"base-financier\", share = 0.25 }]",
            "payment = [{ financierId = \"one\", share = 0.6 }, { financierId = \"two\", share = 0.6 }]",
        );

        match Config::parse_from_str(&toml).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(
                    error,
                    "seeded base on placement placement_1 has financier shares summing to 1.2, expected at most 1"
                );
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("seed with excessive financier shares must fail"),
        }
    }

    #[tokio::test]
    async fn parse_rejects_seeded_trust_without_production_for_resource() {
        let toml = with_structure_seeds().replace("resource = \"Lithium\"", "resource = \"copper\"");

        match Config::parse_from_str(&toml).await {
            Err(Error::ConfigValidation(error)) => {
                assert_eq!(
                    error,
                    "seeded trust on placement placement_2 uses resource copper without configured trust production"
                );
            }
            Err(error) => panic!("unexpected error: {error}"),
            Ok(_) => panic!("trust seed without configured production must fail"),
        }
    }
}
