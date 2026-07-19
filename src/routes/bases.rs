use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::bases;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config
        .service(bases::post)
        .service(bases::list)
        .service(bases::publish_production)
        .service(bases::get)
        .service(bases::delete)
        .service(bases::patch);
}
