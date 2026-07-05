use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::combats;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(combats::list);
}
