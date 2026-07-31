use std::collections::HashMap;

use actix_web::{HttpResponse, Responder, get, web};
use serde::Serialize;

use crate::services::credit_exchange_service::{Cost, CreditExchangeService, Money, ResourceName, Resources};

const COSTS: &str = "costs";

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
struct CostResponse {
    money: Money,
    resources: Resources,
}

impl<T> From<&Cost<T>> for CostResponse {
    fn from(cost: &Cost<T>) -> Self {
        Self {
            money: cost.money(),
            resources: cost.resources_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
struct CostsResponse {
    base: CostResponse,
    unit: CostResponse,
    trusts: HashMap<ResourceName, CostResponse>,
}

impl From<&CreditExchangeService> for CostsResponse {
    fn from(service: &CreditExchangeService) -> Self {
        Self {
            base: (&service.military_base).into(),
            unit: (&service.military_unit).into(),
            trusts: service
                .trust_costs()
                .map(|(resource, cost)| (resource.clone(), cost.into()))
                .collect(),
        }
    }
}

/// List the configured costs for military bases, military units, and every trust type.
#[utoipa::path(
    operation_id = "listCosts",
    tag = COSTS,
    responses(
        (status = 200, description = "All configured costs", body = CostsResponse)
    )
)]
#[get("/costs")]
pub(crate) async fn list(service: web::Data<CreditExchangeService>) -> impl Responder {
    HttpResponse::Ok().json(CostsResponse::from(service.get_ref()))
}

#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};
    use serde_json::json;

    use super::list;
    use crate::{
        domain::LootFactors,
        services::credit_exchange_service::{CreditExchangeService, VecResourceName},
    };

    #[actix_web::test]
    async fn lists_all_configured_costs() {
        let service = CreditExchangeService::new(
            "http://127.0.0.1/".parse().unwrap(),
            "bank".to_string(),
            serde_json::from_value(json!({ "money": 120, "resources": { "water": 4 } })).unwrap(),
            serde_json::from_value(json!({ "money": 1800, "resources": { "steel": 25 } })).unwrap(),
            serde_json::from_value(json!({
                "water": { "money": 600, "resources": { "solar_energy": 15 } },
                "iron": { "money": 700, "resources": { "steel": 10 } }
            }))
            .unwrap(),
            serde_json::from_value::<VecResourceName>(json!(["water", "iron"])).unwrap(),
            LootFactors::default(),
        );
        let app = test::init_service(App::new().app_data(web::Data::new(service)).service(list)).await;

        let request = test::TestRequest::get().uri("/costs").to_request();
        let response: serde_json::Value = test::call_and_read_body_json(&app, request).await;

        assert_eq!(
            response,
            json!({
                "base": { "money": 1800.0, "resources": { "steel": 25.0 } },
                "unit": { "money": 120.0, "resources": { "water": 4.0 } },
                "trusts": {
                    "water": { "money": 600.0, "resources": { "solar_energy": 15.0 } },
                    "iron": { "money": 700.0, "resources": { "steel": 10.0 } }
                }
            })
        );
    }
}
