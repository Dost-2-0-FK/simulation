use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::resources;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(resources::list);
}
