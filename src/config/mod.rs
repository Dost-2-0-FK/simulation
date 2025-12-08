mod error;
mod string_normalization;

use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use tokio::fs;

use self::{
    error::{Error, Result},
    string_normalization::{LowercaseString, VecLowercaseString},
};

const CONFIG_FILE_NAME: &str = "simulation.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Config {
    server: Option<ServerConfig>,
    env: EnvConfig,
    resources: VecLowercaseString,
    costs: CostsConfig,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CostsConfig {
    trust: HashMap<LowercaseString, f64>,
    base: HashMap<LowercaseString, f64>,
}

impl Config {
    pub(crate) async fn parse() -> Result<Self> {
        let config = fs::read_to_string(CONFIG_FILE_NAME).await.map_err(Error::Io)?;
        Self::parse_from_str(&config)
    }

    fn parse_from_str(config: &str) -> Result<Self> {
        let config = toml::from_str::<Config>(config).map_err(Error::Toml)?;

        let resources_in_costs = config.costs.trust.keys().chain(config.costs.base.keys());

        for resource in resources_in_costs {
            // TODO collect all errors in a vector and properly build the error
            assert!(
                config.resources.contains(resource),
                "all resources occuring in costs must be added as resources.
                \"{resource}\" is not contained in {resources}",
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

        [costs]
        base = { lithium = 5.2, iron = 10.5 }
        
        [costs.trust]
        lithium = 1.5
        iron = 2.5

        "#;

        let decoded = Config::parse_from_str(toml_str).expect("config can be parsed");

        assert_eq!(decoded.server.expect("config string contains server").port, 0);
    }
}
