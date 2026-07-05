use utoipa_actix_web::service_config::ServiceConfig;

pub(crate) mod bases;
pub(crate) mod blocs;
pub(crate) mod combats;
pub(crate) mod placements;
pub(crate) mod trusts;
pub(crate) mod units;
pub(crate) mod zones;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config
        .configure(units::configure)
        .configure(bases::configure)
        .configure(placements::configure)
        .configure(trusts::configure)
        .configure(blocs::configure)
        .configure(combats::configure)
        .configure(zones::configure);
}
