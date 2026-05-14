use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::placements;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(placements::list).service(placements::get);
}
