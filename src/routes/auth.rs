use utoipa_actix_web::service_config::ServiceConfig;

use crate::handlers::auth;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config
        .service(auth::login)
        .service(auth::get_current_user)
        .service(auth::logout);
}
