use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    config::Config,
    domain::{BlocName, Loot, MilitaryUnit, Placement, PlacementId, Trust, TrustId, UnitId},
    geometry::{Point, Positioned, WorldBounds},
    handlers::{bases::Financing, trusts::TrustResponse},
    services::credit_exchange_service::{CreditExchangeService, ResourceName, Resources, Share},
};

struct UnitSnapshot {
    position: Point,
    bloc: BlocName,
}

struct PlacementSnapshot {
    id: PlacementId,
    position: Point,
}

async fn unit_snapshots(units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>) -> Vec<UnitSnapshot> {
    let mut snapshots = Vec::with_capacity(units.len());
    for unit in units.values() {
        let unit = unit.read().await;
        snapshots.push(UnitSnapshot {
            position: unit.position(),
            bloc: unit.base().await.bloc_name().clone(),
        });
    }
    snapshots
}

fn placement_snapshots(placements: impl Iterator<Item = Arc<Placement>>) -> Vec<PlacementSnapshot> {
    placements
        .map(|placement| PlacementSnapshot {
            id: placement.id().clone(),
            position: placement.position(),
        })
        .collect()
}

fn inhibition_factor(
    trust: &Trust,
    units: &[UnitSnapshot],
    placements: &[PlacementSnapshot],
    world_bounds: WorldBounds,
    inhibition_radius: f64,
    close_units_factor: Share,
    combat_factor: Share,
) -> Share {
    let trust_bloc = trust.placement().zone().bloc_name();
    let closest_placement_distance = placements
        .iter()
        .filter(|placement| &placement.id != trust.placement_id())
        .map(|placement| world_bounds.distance_between(trust.position(), placement.position))
        .min_by(|left, right| left.partial_cmp(right).expect("distances are finite"));
    let mut close_enemy = false;
    for unit in units.iter().filter(|unit| &unit.bloc != trust_bloc) {
        let distance = world_bounds.distance_between(trust.position(), unit.position);
        if distance == 0.0 {
            return combat_factor;
        }
        let within_placement_cap = closest_placement_distance
            .map(|placement_distance| placement_distance != 0.0 && distance / placement_distance <= 0.5)
            .unwrap_or(true);
        if distance <= inhibition_radius && within_placement_cap {
            close_enemy = true;
        }
    }

    if close_enemy {
        close_units_factor
    } else {
        Share::from(1.0)
    }
}

struct ProductionContext {
    units: Vec<UnitSnapshot>,
    placements: Vec<PlacementSnapshot>,
    world_bounds: WorldBounds,
    inhibition_radius: f64,
    close_units_factor: Share,
    combat_factor: Share,
}

impl ProductionContext {
    async fn new(units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>, config: &Config) -> Self {
        Self {
            units: unit_snapshots(units).await,
            placements: placement_snapshots(config.placements()),
            world_bounds: config.world_bounds(),
            inhibition_radius: config.trust_inhibition_radius(),
            close_units_factor: config.trust_inhibition_factor_close_units(),
            combat_factor: config.trust_inhibition_factor_combat(),
        }
    }

    fn production_for(&self, trust: &Trust) -> Resources {
        let factor = inhibition_factor(
            trust,
            &self.units,
            &self.placements,
            self.world_bounds,
            self.inhibition_radius,
            self.close_units_factor,
            self.combat_factor,
        );
        trust.production_with_inhibition(factor)
    }
}

pub(crate) async fn get_all(
    resp: Sender<Vec<TrustResponse>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) {
    let context = ProductionContext::new(units, config).await;
    let mut result = Vec::with_capacity(trusts.len());
    for trust in trusts.values() {
        let trust = trust.read().await;
        result.push(TrustResponse::new(&trust, context.production_for(&trust)));
    }
    let _ = resp.send(result);
}

pub(crate) async fn get(
    id: TrustId,
    resp: Sender<Option<TrustResponse>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) {
    let context = ProductionContext::new(units, config).await;
    let trust = match trusts.get(&id) {
        Some(trust) => {
            let trust = trust.read().await;
            Some(TrustResponse::new(&trust, context.production_for(&trust)))
        }
        None => None,
    };
    let _ = resp.send(trust);
}

pub(crate) async fn create(
    placement_id: PlacementId,
    financing: Vec<Financing>,
    resource: ResourceName,
    trust_production_income: &Loot,
    credit_exchange_service: &CreditExchangeService,
    mut placements: impl Iterator<Item = Arc<Placement>>,
) -> Result<Trust, CommandError> {
    log::debug!("received command to create trust on placement with id {placement_id:?}");
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    let Some(resource_amount) = trust_production_income.resource_amount(&resource) else {
        return Err(CommandError::NotFound("Resource"));
    };
    let payment = credit_exchange_service
        .pay_for_trust(placement.zone().name(), financing)
        .await
        .map_err(CommandError::CreditExchange)?;
    let payment_policy = payment.policy().clone();
    let trust = Trust::new(
        payment,
        credit_exchange_service.loot_factors(),
        placement,
        resource,
        resource_amount,
        trust_production_income.money(),
    );
    credit_exchange_service
        .register_trust(&trust, &payment_policy)
        .await
        .map_err(CommandError::CreditExchange)?;
    Ok(trust)
}

pub(crate) async fn publish_production(
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) -> anyhow::Result<()> {
    let context = ProductionContext::new(units, config).await;
    for trust_arc in trusts.values() {
        let trust = trust_arc.read().await;
        let producing = context.production_for(&trust);

        if let Err(err) = config
            .credit_exchange_service()
            .set_trust_production(&trust, &producing)
            .await
        {
            log::error!(
                "failed to publish credit production for trust {trust_id:?}: {err}",
                trust_id = trust.id()
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use ordered_float::NotNan;

    use super::*;
    use crate::{
        domain::{Bloc, Chance, LootFactors, Zone, ZoneName},
        services::credit_exchange_service::{Cost, Money},
    };

    fn point(x: f64, y: f64) -> Point {
        Point::new(NotNan::new(x).unwrap(), NotNan::new(y).unwrap())
    }

    fn world_bounds() -> WorldBounds {
        serde_json::from_value(serde_json::json!({
            "min_x": 0.0,
            "max_x": 30.0,
            "min_y": 0.0,
            "max_y": 30.0
        }))
        .unwrap()
    }

    fn trust(position: Point) -> Trust {
        let bloc_name = BlocName::from("trust-bloc".to_string());
        let bloc = Arc::new(RwLock::new(Bloc::new(
            bloc_name.clone(),
            Chance::new(1),
            Share::default(),
        )));
        let zone = Arc::new(Zone::new(ZoneName::from("trust-zone".to_string()), bloc_name, bloc));
        let placement = Arc::new(Placement::new(
            serde_json::from_str(r#""trust-placement""#).unwrap(),
            zone,
            position,
        ));
        let cost: Cost<Trust> = serde_json::from_value(serde_json::json!({
            "money": 0.0,
            "resources": {}
        }))
        .unwrap();

        Trust::new_prepaid(
            vec![],
            &cost,
            &LootFactors::default(),
            placement,
            ResourceName::new("iron".to_string()),
            8.0,
            Money::from(2.0),
        )
    }

    fn unit(position: Point, bloc: &str) -> UnitSnapshot {
        UnitSnapshot {
            position,
            bloc: BlocName::from(bloc.to_string()),
        }
    }

    fn placement(id: &str, position: Point) -> PlacementSnapshot {
        PlacementSnapshot {
            id: serde_json::from_str(&format!(r#""{id}""#)).unwrap(),
            position,
        }
    }

    fn distant_placement() -> [PlacementSnapshot; 1] {
        [placement("other-placement", point(20.0, 10.0))]
    }

    #[test]
    fn production_is_not_inhibited_by_friendly_or_distant_units() {
        let trust = trust(point(10.0, 10.0));
        let units = [
            unit(point(10.5, 10.0), "trust-bloc"),
            unit(point(12.0, 10.0), "enemy-bloc"),
        ];

        let factor = inhibition_factor(
            &trust,
            &units,
            &distant_placement(),
            world_bounds(),
            1.0,
            Share::from(0.75),
            Share::from(0.25),
        );

        assert_eq!(factor, Share::from(1.0));
    }

    #[test]
    fn close_enemy_inhibits_production_across_wrapped_world_edge() {
        let trust = trust(point(0.25, 10.0));
        let units = [unit(point(29.5, 10.0), "enemy-bloc")];

        let factor = inhibition_factor(
            &trust,
            &units,
            &distant_placement(),
            world_bounds(),
            1.0,
            Share::from(0.75),
            Share::from(0.25),
        );

        assert_eq!(factor, Share::from(0.75));
    }

    #[test]
    fn enemy_at_trust_uses_combat_factor_and_scales_resources() {
        let trust = trust(point(10.0, 10.0));
        let units = [
            unit(point(10.5, 10.0), "enemy-bloc"),
            unit(point(10.0, 10.0), "other-enemy-bloc"),
        ];

        let factor = inhibition_factor(
            &trust,
            &units,
            &distant_placement(),
            world_bounds(),
            1.0,
            Share::from(0.75),
            Share::from(0.25),
        );
        let producing = trust.production_with_inhibition(factor);

        assert_eq!(factor, Share::from(0.25));
        assert_eq!(producing.get(&ResourceName::new("iron".to_string())), Some(2.0));
        assert_eq!(
            trust.producing_base_value().get(&ResourceName::new("iron".to_string())),
            Some(8.0)
        );
    }

    #[test]
    fn closest_placement_caps_inhibition_radius_at_half_its_distance() {
        let trust = trust(point(1.0, 10.0));
        let placements = [placement("other-placement", point(29.0, 10.0))];

        let outside_capped_radius = inhibition_factor(
            &trust,
            &[unit(point(2.5, 10.0), "enemy-bloc")],
            &placements,
            world_bounds(),
            10.0,
            Share::from(0.75),
            Share::from(0.25),
        );
        let on_capped_radius = inhibition_factor(
            &trust,
            &[unit(point(2.0, 10.0), "enemy-bloc")],
            &placements,
            world_bounds(),
            10.0,
            Share::from(0.75),
            Share::from(0.25),
        );

        assert_eq!(outside_capped_radius, Share::from(1.0));
        assert_eq!(on_capped_radius, Share::from(0.75));
    }
}
