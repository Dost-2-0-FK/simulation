mod error;

use std::time::Duration;

use serde::Deserialize;
use serde_with::{DurationSecondsWithFrac, serde_as};
use tokio::fs;

use self::error::{Error, Result};
use crate::{
    military::{MilitaryBase, MilitaryUnit},
    payment_service::{Cost, VecResourceName},
    trust::Trust,
};

const CONFIG_FILE_NAME: &str = "simulation.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    server: Option<ServerConfig>,
    env: EnvConfig,
    resources: VecResourceName,
    time: TimeConfig,
    costs: CostsConfig,
}

impl Config {
    pub(crate) fn costs(&self) -> &CostsConfig {
        &self.costs
    }
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
    port: u64,
}

#[serde_as]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeConfig {
    #[serde(rename = "main_loop_tick_seconds")]
    #[serde_as(as = "DurationSecondsWithFrac<f64>")]
    main_loop_tick: Duration,
    combat_loop_tick_factor: u8,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CostsConfig {
    trust: Cost<Trust>,
    base: Cost<MilitaryBase>,
    unit: Cost<MilitaryUnit>,
}

impl CostsConfig {
    pub(crate) fn base(&self) -> &Cost<MilitaryBase> {
        &self.base
    }

    pub(crate) fn unit(&self) -> &Cost<MilitaryUnit> {
        &self.unit
    }
}

impl Config {
    pub(crate) async fn parse() -> Result<Self> {
        let config = fs::read_to_string(CONFIG_FILE_NAME).await.map_err(Error::Io)?;
        Self::parse_from_str(&config)
    }

    fn parse_from_str(config: &str) -> Result<Self> {
        let config = toml::from_str::<Config>(config).map_err(Error::Toml)?;

        let resources_in_costs = config
            .costs
            .trust
            .resources()
            .chain(config.costs.base.resources())
            .chain(config.costs.unit.resources());

        for resource_value in resources_in_costs {
            // TODO collect all errors in a vector and properly build the error
            assert!(
                config.resources.contains(resource_value.name()),
                "all resources occuring in costs must be added as resources.
                \"{resource_value}\" is not contained in {resources}",
                resources = config.resources
            );
        }

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let toml_str = r#"
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

        [costs]
        base = { money = 1.5, resources = { lithium = 5.2, iron = 10.5 } }
        unit = { money = 1.5, resources = { lithium = 5.2, iron = 10.5 } }
        
        [costs.trust.resources]
        lithium = 1.5
        iron = 2.5

        [costs.trust]
        money = 1.2

        "#;

        let decoded = Config::parse_from_str(toml_str).expect("config can be parsed");

        assert_eq!(decoded.server.expect("config string contains server").port, 0);
    }
}
