use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::zones;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(zones::list).service(zones::patch);
}
