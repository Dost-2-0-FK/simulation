use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::trusts;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config
        .service(trusts::post)
        .service(trusts::publish_production)
        .service(trusts::list)
        .service(trusts::get)
        .service(trusts::delete);
}
