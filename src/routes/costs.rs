use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::costs;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(costs::list);
}
