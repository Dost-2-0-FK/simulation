pub(crate) mod base;
pub(crate) mod bloc;
pub(crate) mod combat;
mod deletion;
pub(crate) mod persist;
pub(crate) mod placement;
pub(crate) mod production_unit;
pub(crate) mod trust;
pub(crate) mod unit;
pub(crate) mod zone;

#[derive(Debug)]
pub(crate) enum CommandError {
    NotFound(&'static str),
    CreditExchange(anyhow::Error),
}

impl CommandError {
    fn into_user_error(self, action: &str) -> UserError {
        match self {
            Self::NotFound(name) => UserError::NotFound(name),
            Self::CreditExchange(error) => match error.downcast_ref::<CreditExchangeResponseError>() {
                Some(response) if response.is_insufficient_credit() => {
                    UserError::PaymentRequired(response.body().to_string())
                }
                Some(response) => UserError::CreditExchange {
                    status: response.status().as_u16(),
                    body: response.body().to_string(),
                },
                None => {
                    log::error!("credit exchange error while {action}: {error:#}");
                    UserError::InternalError
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CommandError;
    use crate::{error::UserError, services::credit_exchange_service::CreditExchangeResponseError};

    #[test]
    fn credit_exchange_http_response_is_forwarded_as_user_error() {
        let upstream = CreditExchangeResponseError::new(
            reqwest::StatusCode::BAD_REQUEST,
            "Insufficient credit for booking".to_string(),
        );

        let error = CommandError::CreditExchange(upstream.into()).into_user_error("creating base");

        assert!(matches!(
            error,
            UserError::PaymentRequired(body)
                if body == "Insufficient credit for booking"
        ));
    }

    #[test]
    fn other_credit_exchange_bad_requests_remain_bad_requests() {
        let upstream =
            CreditExchangeResponseError::new(reqwest::StatusCode::BAD_REQUEST, "Invalid credit type".to_string());

        let error = CommandError::CreditExchange(upstream.into()).into_user_error("creating trust");

        assert!(matches!(
            error,
            UserError::CreditExchange { status: 400, body }
                if body == "Invalid credit type"
        ));
    }
}

use std::{collections::HashMap, sync::Arc};

use tokio::sync::{Mutex, RwLock, mpsc::Receiver, oneshot::Sender};

use crate::{
    config::Config,
    domain::{
        BaseId, Bloc, BlocKey, Chance, Combat, MilitaryBase, MilitaryUnit, Placement, PlacementId, Target, Trust,
        ProductionUnit, ProductionUnitKey, SocialRuleLevel, SocialRuleName, TrustId, UnitId, Zone, ZoneKey,
    },
    error::UserError,
    geometry::Point,
    handlers::{
        bases::{Financing, TargetBody},
        combats::CombatResponse,
        production_units::ProductionUnitResponse,
        trusts::TrustResponse,
        units::UnitResponse,
    },
    persistence::MongoPersistence,
    services::credit_exchange_service::{CreditExchangeResponseError, ResourceName, Share},
};

/// Used to query or mutate the state of the [state_loop].
#[derive(Debug)]
pub(crate) enum Command {
    GetUnits(Sender<core::result::Result<Vec<UnitResponse>, UserError>>),
    GetCombats(Sender<core::result::Result<Vec<CombatResponse>, UserError>>),
    CreateBase {
        placement_id: PlacementId,
        financing: Vec<Financing>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetBases(Sender<Vec<MilitaryBase>>),
    GetBase(BaseId, Sender<Option<MilitaryBase>>),
    PatchBase {
        id: BaseId,
        enabled: Option<bool>,
        prioritized: Option<bool>,
        target: Option<TargetBody>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    DeleteBase {
        id: BaseId,
        response: Sender<core::result::Result<(), UserError>>,
    },
    CreateTrust {
        placement_id: PlacementId,
        financing: Vec<Financing>,
        resource: ResourceName,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetTrusts(Sender<core::result::Result<Vec<TrustResponse>, UserError>>),
    GetTrust(TrustId, Sender<core::result::Result<Option<TrustResponse>, UserError>>),
    GetProductionUnits(Sender<core::result::Result<Vec<ProductionUnitResponse>, UserError>>),
    GetProductionUnit(
        ProductionUnitKey,
        Sender<core::result::Result<Option<ProductionUnitResponse>, UserError>>,
    ),
    DeleteTrust {
        id: TrustId,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetPlacements(Sender<Vec<Arc<Placement>>>),
    GetZones(Sender<Vec<Arc<Zone>>>),
    PatchZone {
        id: ZoneKey,
        social_rules: Vec<(SocialRuleName, SocialRuleLevel)>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    GetBlocs(Sender<Vec<Bloc>>),
    PatchBloc {
        id: BlocKey,
        chance: Option<Chance>,
        military_expense: Option<Share>,
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Persist the current in-memory state to the database. Sent periodically by a background task.
    Persist,
    /// Publish accumulated base loot to the credit service.
    PublishBaseProduction {
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Publish trust production to the credit service.
    PublishTrustProduction {
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Run a military unit production cycle for all blocs.
    ProduceMilitaryUnits {
        response: Sender<core::result::Result<(), UserError>>,
    },
    /// Move all military units one step toward their closest enemy target. Sent periodically by a background task.
    MoveMilitaryUnits,
    /// On all combats, execute a [Combat::tick].
    CombatTick,
}

/// The core of the state is this loop, where it accepts commands to be read or mutated.
#[expect(clippy::too_many_arguments)]
pub(crate) async fn run(
    mut receiver: Receiver<Command>,
    config: &Config,
    persistence: &MongoPersistence,
    mut units: HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    mut bases: HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    mut trusts: HashMap<TrustId, Arc<RwLock<Trust>>>,
    production_units: HashMap<ProductionUnitKey, Arc<RwLock<ProductionUnit>>>,
    blocs: HashMap<BlocKey, Arc<RwLock<Bloc>>>,
    mut combats: HashMap<Point, Arc<RwLock<Combat>>>,
) {
    // We need this because combat tick and combat initiation should never happen concurrently.
    let combat_lock = Mutex::new(());

    while let Some(cmd) = receiver.recv().await {
        match cmd {
            Command::GetUnits(resp) => {
                unit::get(resp, &units, config).await;
            }
            Command::GetCombats(resp) => {
                combat::get_all(resp, &combats, config).await;
            }
            Command::CreateBase {
                placement_id,
                financing,
                response,
            } => {
                let result = async {
                    if placement_is_occupied(&placement_id, &bases, &trusts).await {
                        return Err(UserError::Conflict("Placement"));
                    }

                    let approved = config
                        .auth_service()
                        .verify_financing(
                            &placement_id,
                            crate::services::auth_service::FinancedObject::Base,
                            &financing,
                            None,
                        )
                        .await
                        .map_err(|err| {
                            log::error!("auth-service error while verifying base financing: {err:#}");
                            UserError::InternalError
                        })?;
                    if !approved {
                        return Err(UserError::Forbidden);
                    }

                    let base = base::create(
                        placement_id,
                        financing,
                        config.credit_exchange_service(),
                        config.placements(),
                    )
                    .await
                    .map_err(|err| err.into_user_error("creating base"))?;
                    bases.insert(base.id(), Arc::new(RwLock::new(base)));
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::GetBases(resp) => {
                base::get_all(resp, &bases).await;
            }
            Command::GetBase(id, resp) => {
                base::get(id, resp, &bases).await;
            }
            Command::PatchBase {
                id,
                enabled,
                prioritized,
                target,
                response,
            } => {
                let result = async {
                    let resolved_target = match target {
                        None => None,
                        Some(TargetBody::None) => Some(Target::None),
                        Some(TargetBody::Base { id: base_id }) => {
                            let arc = bases.get(&base_id).ok_or(UserError::NotFound("Base"))?;
                            Some(Target::Base {
                                id: base_id,
                                base: arc.clone(),
                            })
                        }
                        Some(TargetBody::Trust { id: trust_id }) => {
                            let arc = trusts.get(&trust_id).ok_or(UserError::NotFound("Trust"))?;
                            Some(Target::Trust {
                                id: trust_id,
                                trust: arc.clone(),
                            })
                        }
                    };
                    let lock = bases.get(&id).ok_or(UserError::NotFound("Base"))?;
                    let patched = base::patch(lock.read().await.clone(), enabled, prioritized, resolved_target);
                    *lock.write().await = patched;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::DeleteBase { id, response } => {
                let result = deletion::delete_base(
                    id,
                    &mut bases,
                    &mut units,
                    &mut combats,
                    config.credit_exchange_service(),
                )
                .await;
                let _ = response.send(result);
            }
            Command::CreateTrust {
                placement_id,
                financing,
                resource,
                response,
            } => {
                let result = async {
                    if placement_is_occupied(&placement_id, &bases, &trusts).await {
                        return Err(UserError::Conflict("Placement"));
                    }

                    let approved = config
                        .auth_service()
                        .verify_financing(
                            &placement_id,
                            crate::services::auth_service::FinancedObject::Trust,
                            &financing,
                            Some(&resource),
                        )
                        .await
                        .map_err(|err| {
                            log::error!("auth-service error while verifying trust financing: {err:#}");
                            UserError::InternalError
                        })?;
                    if !approved {
                        return Err(UserError::Forbidden);
                    }

                    let Some(resource_amount) = config.trust_resource_production(&resource) else {
                        return Err(UserError::NotFound("Resource"));
                    };
                    let Some(base_income) = config.trust_base_income(&resource) else {
                        return Err(UserError::NotFound("Resource"));
                    };

                    let trust = trust::create(
                        placement_id,
                        financing,
                        resource,
                        resource_amount,
                        base_income,
                        config.credit_exchange_service(),
                        config.placements(),
                    )
                    .await
                    .map_err(|err| err.into_user_error("creating trust"))?;
                    trusts.insert(trust.id(), Arc::new(RwLock::new(trust)));
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::GetTrusts(resp) => {
                trust::get_all(resp, &trusts, &units, config).await;
            }
            Command::GetTrust(id, resp) => {
                trust::get(id, resp, &trusts, &units, config).await;
            }
            Command::GetProductionUnits(response) => {
                production_unit::get_all(response, &production_units, config.credit_exchange_service()).await;
            }
            Command::GetProductionUnit(key, response) => {
                production_unit::get(
                    &key,
                    response,
                    &production_units,
                    config.credit_exchange_service(),
                )
                .await;
            }
            Command::DeleteTrust { id, response } => {
                let result = deletion::delete_trust(
                    id,
                    &mut bases,
                    &mut trusts,
                    &mut combats,
                    config.credit_exchange_service(),
                )
                .await;
                let _ = response.send(result);
            }
            Command::GetPlacements(resp) => {
                placement::get(resp, config.placements());
            }
            Command::GetZones(resp) => {
                zone::get(resp, config.zones());
            }
            Command::PatchZone {
                id,
                social_rules,
                response,
            } => {
                zone::patch(
                    response,
                    config.zones(),
                    config.auth_service(),
                    &id,
                    &social_rules,
                )
                .await;
            }
            Command::GetBlocs(resp) => {
                bloc::get_all(resp, &blocs).await;
            }
            Command::PatchBloc {
                id,
                chance,
                military_expense,
                response,
            } => {
                let result = async {
                    let lock = blocs.get(&id).ok_or(UserError::NotFound("Bloc"))?;
                    let patched = bloc::patch(&lock.read().await.clone(), chance, military_expense);
                    *lock.write().await = patched;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::Persist => {
                persist::persist_all(
                    persistence,
                    &units,
                    &bases,
                    &trusts,
                    &production_units,
                    &blocs,
                    &combats,
                    config.zones(),
                )
                .await;
            }
            Command::PublishBaseProduction { response } => {
                let result = async {
                    base::publish_production(&bases, config.credit_exchange_service())
                        .await
                        .map_err(|err| {
                            log::error!("failed to publish base production: {err:#}");
                            UserError::InternalError
                        })?;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::PublishTrustProduction { response } => {
                let result = async {
                    trust::publish_production(&trusts, &production_units, &units, config)
                        .await
                        .map_err(|err| {
                            log::error!("failed to publish trust production: {err:#}");
                            UserError::CreditExchangeQueryFailed
                        })?;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::ProduceMilitaryUnits { response } => {
                let result = async {
                    unit::produce_units(&blocs, &bases, &mut units, config.credit_exchange_service())
                        .await
                        .map_err(|err| {
                            log::error!("failed to produce military units: {err:#}");
                            UserError::InternalError
                        })?;
                    Ok(())
                }
                .await;
                let _ = response.send(result);
            }
            Command::MoveMilitaryUnits => {
                let _combat_lock_guard = combat_lock.lock().await;
                combat::clear_dead_units(&mut units).await;
                unit::move_units(
                    &mut units,
                    &mut combats,
                    config.movement_step(),
                    config.world_bounds(),
                    config.base_destruction_threshold(),
                    config.trust_destruction_threshold(),
                )
                .await;
            }
            Command::CombatTick => {
                let _combat_lock_guard = combat_lock.lock().await;
                let events = combat::tick(&mut combats).await;
                combat::apply_events(&events, &mut bases, &mut trusts).await;
                combat::clear_dead_units(&mut units).await;
            }
        }
    }
}

async fn placement_is_occupied(
    placement_id: &PlacementId,
    bases: &HashMap<BaseId, Arc<RwLock<MilitaryBase>>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
) -> bool {
    for base in bases.values() {
        if base.read().await.placement_id() == placement_id {
            return true;
        }
    }

    for trust in trusts.values() {
        if trust.read().await.placement_id() == placement_id {
            return true;
        }
    }

    false
}
