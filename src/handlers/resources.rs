use actix_web::{HttpResponse, Responder, get, web};

use crate::services::credit_exchange_service::ResourceName;

const RESOURCES: &str = "resources";

/// List configured resources that are not produced by production units.
#[utoipa::path(
    operation_id = "listResources",
    tag = RESOURCES,
    responses(
        (status = 200, description = "Configured resources not produced by production units", body = [ResourceName])
    )
)]
#[get("/resources")]
pub(crate) async fn list(resources: web::Data<Vec<ResourceName>>) -> impl Responder {
    HttpResponse::Ok().json(resources.get_ref())
}

#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};

    use super::list;
    use crate::services::credit_exchange_service::ResourceName;

    #[actix_web::test]
    async fn lists_available_resources() {
        let resources = vec![
            ResourceName::new("Lithium".to_string()),
            ResourceName::new("Iron".to_string()),
        ];
        let app = test::init_service(App::new().app_data(web::Data::new(resources)).service(list)).await;

        let request = test::TestRequest::get().uri("/resources").to_request();
        let response: Vec<String> = test::call_and_read_body_json(&app, request).await;

        assert_eq!(response, ["lithium", "iron"]);
    }
}
