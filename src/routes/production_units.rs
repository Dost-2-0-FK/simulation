use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::production_units;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(production_units::list).service(production_units::get);
}
