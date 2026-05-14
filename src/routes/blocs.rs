use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::blocs;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config.service(blocs::list).service(blocs::get).service(blocs::patch);
}
