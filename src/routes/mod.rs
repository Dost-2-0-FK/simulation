use utoipa_actix_web::service_config::ServiceConfig;

pub(crate) mod auth;
pub(crate) mod bases;
pub(crate) mod blocs;
pub(crate) mod combats;
pub(crate) mod costs;
pub(crate) mod placements;
pub(crate) mod production_units;
pub(crate) mod resources;
pub(crate) mod stats;
pub(crate) mod trusts;
pub(crate) mod units;
pub(crate) mod zones;

pub(crate) fn configure(config: &mut ServiceConfig<'_>) {
    config
        .configure(auth::configure)
        .configure(units::configure)
        .configure(bases::configure)
        .configure(placements::configure)
        .configure(production_units::configure)
        .configure(resources::configure)
        .configure(costs::configure)
        .configure(stats::configure)
        .configure(trusts::configure)
        .configure(blocs::configure)
        .configure(combats::configure)
        .configure(zones::configure);
}
