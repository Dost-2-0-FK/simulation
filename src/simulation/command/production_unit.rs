use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use crate::{
    domain::{ProductionUnit, ProductionUnitKey},
    error::UserError,
    handlers::production_units::ProductionUnitResponse,
    services::credit_exchange_service::CreditExchangeService,
};

pub(crate) async fn get_all(
    response: Sender<core::result::Result<Vec<ProductionUnitResponse>, UserError>>,
    production_units: &HashMap<ProductionUnitKey, Arc<RwLock<ProductionUnit>>>,
    credit_exchange_service: &CreditExchangeService,
) {
    let result = async {
        if production_units.is_empty() {
            return Ok(Vec::new());
        }
        let resource_totals = credit_exchange_service
            .resource_totals_excluding_bank()
            .await
            .map_err(|error| {
                log::error!("failed to query credit service while listing production units: {error:#}");
                UserError::CreditExchangeQueryFailed
            })?;

        let mut result = Vec::with_capacity(production_units.len());
        for production_unit in production_units.values() {
            let production_unit = production_unit.read().await;
            result.push(response_for(&production_unit, &resource_totals));
        }
        Ok(result)
    }
    .await;
    let _ = response.send(result);
}

pub(crate) async fn get(
    key: &ProductionUnitKey,
    response: Sender<core::result::Result<Option<ProductionUnitResponse>, UserError>>,
    production_units: &HashMap<ProductionUnitKey, Arc<RwLock<ProductionUnit>>>,
    credit_exchange_service: &CreditExchangeService,
) {
    let result = async {
        let Some(production_unit) = production_units.get(key) else {
            return Ok(None);
        };
        let resource_totals = credit_exchange_service
            .resource_totals_excluding_bank()
            .await
            .map_err(|error| {
                log::error!("failed to query credit service while getting production unit {key}: {error:#}");
                UserError::CreditExchangeQueryFailed
            })?;
        let production_unit = production_unit.read().await;
        Ok(Some(response_for(&production_unit, &resource_totals)))
    }
    .await;
    let _ = response.send(result);
}

fn response_for(
    production_unit: &ProductionUnit,
    resource_totals: &crate::services::credit_exchange_service::Resources,
) -> ProductionUnitResponse {
    let producing = production_unit.production_without_inhibition();
    let existing_units = resource_totals.get(production_unit.resource_name()).unwrap_or_default();
    let income = production_unit.income(
        producing
            .into_iter()
            .next()
            .expect("production units produce one resource"),
        existing_units,
    );
    ProductionUnitResponse::new(production_unit, income, producing)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::RwLock;

    use super::response_for;
    use crate::{
        domain::{Bloc, BlocKey, BlocName, Chance, ProductionUnit, ProductionUnitKey, Zone, ZoneKey, ZoneName},
        services::credit_exchange_service::{Money, ResourceName, Resources, Share},
    };

    #[test]
    fn response_uses_uninhibited_production_and_zero_configured_income() {
        let bloc_key = BlocKey::from("bloc".to_string());
        let bloc_name = BlocName::from("Bloc".to_string());
        let zone = Arc::new(Zone::new(
            ZoneKey::from("zone".to_string()),
            ZoneName::from("Zone".to_string()),
            bloc_key.clone(),
            bloc_name.clone(),
            Arc::new(RwLock::new(Bloc::new(
                bloc_key,
                bloc_name,
                Chance::new(1),
                Share::default(),
            ))),
        ));
        let production_unit = ProductionUnit::new(
            serde_json::from_str::<ProductionUnitKey>(r#""humans-zone""#).unwrap(),
            zone,
            ResourceName::new("humans".to_string()),
            1.0,
            Money::from(0.0),
        );

        let response = serde_json::to_value(response_for(&production_unit, &Resources::default())).unwrap();

        assert_eq!(response["income"], 0.0);
        assert_eq!(response["producing"]["humans"], 1.0);
    }
}
