mod error;

use std::{collections::HashMap, path::Path};

use serde::Deserialize;
use tokio::fs;

use self::{
    error::{Error, Result},
};
use crate::{
    military::MilitaryUnitCost,
    money::{Costs, Money},
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
    trust: CostConfig,
    base: CostConfig,
    unit: CostConfig,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CostConfig {
    money: Money,
    resources: HashMap<LowercaseString, f64>,
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
            .resources
            .keys()
            .chain(config.costs.base.resources.keys())
            .chain(config.costs.unit.resources.keys());

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

    pub(crate) fn costs(&self) -> Costs {
        let military_unit_cost = MilitaryUnitCost {
            money: self.costs.unit.money,
            resource: HashMap::new(),
        };
        Costs {
            military_unit: military_unit_cost,
        }
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
