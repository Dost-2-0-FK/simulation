use std::{collections::HashMap, sync::Arc};

use tokio::sync::{RwLock, oneshot::Sender};

use super::CommandError;
use crate::{
    config::Config,
    domain::{
        BlocKey, MilitaryUnit, Placement, PlacementId, ProductionUnit, ProductionUnitKey, Trust, TrustId, UnitId,
    },
    error::UserError,
    geometry::{Distance, Point, Positioned, WorldBounds},
    handlers::{bases::Financing, trusts::TrustResponse},
    services::credit_exchange_service::{CreditExchangeService, Money, ResourceName, ResourceValue, Resources, Share},
};

struct UnitSnapshot {
    position: Point,
    bloc: BlocKey,
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
            bloc: unit.base().await.bloc_key().clone(),
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
    world_bounds: WorldBounds,
    inhibition_radius: Distance,
    close_units_factor: Share,
    combat_factor: Share,
) -> Share {
    let trust_bloc = trust.placement().zone().bloc_key();
    let mut close_enemy = false;
    for unit in units.iter().filter(|unit| &unit.bloc != trust_bloc) {
        let distance = world_bounds.distance_between(trust.position(), unit.position);
        if distance == 0.0 {
            return combat_factor;
        }
        if distance <= inhibition_radius {
            close_enemy = true;
        }
    }

    if close_enemy {
        close_units_factor
    } else {
        Share::from(1.0)
    }
}

fn applied_inhibition_radius(
    trust: &Trust,
    placements: &[PlacementSnapshot],
    world_bounds: WorldBounds,
    configured_radius: Distance,
) -> Distance {
    placements
        .iter()
        .filter(|placement| &placement.id != trust.placement_id())
        .map(|placement| world_bounds.distance_between(trust.position(), placement.position) * 0.5)
        .fold(
            configured_radius,
            |radius, candidate| {
                if candidate < radius { candidate } else { radius }
            },
        )
}

struct ProductionContext {
    units: Vec<UnitSnapshot>,
    placements: Vec<PlacementSnapshot>,
    world_bounds: WorldBounds,
    inhibition_radius: Distance,
    close_units_factor: Share,
    combat_factor: Share,
    resource_totals: Resources,
}

impl ProductionContext {
    async fn new(units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>, config: &Config) -> anyhow::Result<Self> {
        Ok(Self {
            units: unit_snapshots(units).await,
            placements: placement_snapshots(config.placements()),
            world_bounds: config.world_bounds(),
            inhibition_radius: config.trust_inhibition_radius(),
            close_units_factor: config.trust_inhibition_factor_close_units(),
            combat_factor: config.trust_inhibition_factor_combat(),
            resource_totals: config
                .credit_exchange_service()
                .resource_totals_excluding_bank()
                .await?,
        })
    }

    async fn production_for(&self, trust: &Trust) -> (Resources, Distance) {
        let inhibition_radius =
            applied_inhibition_radius(trust, &self.placements, self.world_bounds, self.inhibition_radius);
        let factor = inhibition_factor(
            trust,
            &self.units,
            self.world_bounds,
            inhibition_radius,
            self.close_units_factor,
            self.combat_factor,
        );
        (trust.production_with_inhibition(factor).await, inhibition_radius)
    }

    fn income_for(&self, trust: &Trust, produced: ResourceValue<'_>) -> Money {
        let existing_units = self.resource_totals.get(trust.resource_name()).unwrap_or_default();
        trust.income(produced, existing_units)
    }
}

pub(crate) async fn get_all(
    resp: Sender<core::result::Result<Vec<TrustResponse>, UserError>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) {
    let result = async {
        if trusts.is_empty() {
            return Ok(Vec::new());
        }
        let context = ProductionContext::new(units, config).await.map_err(|err| {
            log::error!("failed to query credit service while listing trusts: {err:#}");
            UserError::CreditExchangeQueryFailed
        })?;
        let mut result = Vec::with_capacity(trusts.len());
        for trust in trusts.values() {
            let trust = trust.read().await;
            let (producing, inhibition_radius) = context.production_for(&trust).await;
            result.push(TrustResponse::new(
                &trust,
                context.income_for(
                    &trust,
                    producing.into_iter().next().expect("trusts produce one resource"),
                ),
                producing,
                inhibition_radius,
                config.name_mappings().as_ref(),
            )?);
        }
        Ok(result)
    }
    .await;
    let _ = resp.send(result);
}

pub(crate) async fn get(
    id: TrustId,
    resp: Sender<core::result::Result<Option<TrustResponse>, UserError>>,
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) {
    let result = async {
        let Some(trust) = trusts.get(&id) else {
            return Ok(None);
        };
        let context = ProductionContext::new(units, config).await.map_err(|err| {
            log::error!("failed to query credit service while getting trust {id:?}: {err:#}");
            UserError::CreditExchangeQueryFailed
        })?;
        let trust = trust.read().await;
        let (producing, inhibition_radius) = context.production_for(&trust).await;
        Ok(Some(TrustResponse::new(
            &trust,
            context.income_for(
                &trust,
                producing.into_iter().next().expect("trusts produce one resource"),
            ),
            producing,
            inhibition_radius,
            config.name_mappings().as_ref(),
        )?))
    }
    .await;
    let _ = resp.send(result);
}

pub(crate) async fn create(
    placement_id: PlacementId,
    financing: Vec<Financing>,
    resource: ResourceName,
    resource_amount: f32,
    base_income: Money,
    credit_exchange_service: &CreditExchangeService,
    mut placements: impl Iterator<Item = Arc<Placement>>,
) -> Result<Trust, CommandError> {
    log::debug!("received command to create trust on placement with id {placement_id:?}");
    let Some(placement) = placements.find(|p| p.id() == &placement_id) else {
        return Err(CommandError::NotFound("Placement"));
    };
    let payment = credit_exchange_service
        .pay_for_trust(placement.zone().key(), financing)
        .await
        .map_err(CommandError::CreditExchange)?;
    let payment_policy = payment.policy().clone();
    let trust = Trust::new(
        payment,
        credit_exchange_service.loot_factors(),
        placement,
        resource,
        resource_amount,
        base_income,
    );
    credit_exchange_service
        .register_trust(&trust, &payment_policy)
        .await
        .map_err(CommandError::CreditExchange)?;
    Ok(trust)
}

pub(crate) async fn publish_production(
    trusts: &HashMap<TrustId, Arc<RwLock<Trust>>>,
    production_units: &HashMap<ProductionUnitKey, Arc<RwLock<ProductionUnit>>>,
    units: &HashMap<UnitId, Arc<RwLock<MilitaryUnit>>>,
    config: &Config,
) -> anyhow::Result<()> {
    if trusts.is_empty() && production_units.is_empty() {
        return Ok(());
    }
    let context = ProductionContext::new(units, config).await?;
    for trust_arc in trusts.values() {
        let trust = trust_arc.read().await;
        let (producing, _) = context.production_for(&trust).await;
        let income = context.income_for(
            &trust,
            producing.into_iter().next().expect("trusts produce one resource"),
        );

        if let Err(err) = config
            .credit_exchange_service()
            .set_trust_production(&trust, income, &producing)
            .await
        {
            log::error!(
                "failed to publish credit production for trust {trust_id:?}: {err}",
                trust_id = trust.id()
            );
        }
    }

    for production_unit in production_units.values() {
        let production_unit = production_unit.read().await;
        let producing = production_unit.production_without_inhibition().await;
        let existing_units = context
            .resource_totals
            .get(production_unit.resource_name())
            .unwrap_or_default();
        let income = production_unit.income(
            producing
                .into_iter()
                .next()
                .expect("production units produce one resource"),
            existing_units,
        );

        if let Err(err) = config
            .credit_exchange_service()
            .set_production_unit_production(&production_unit, income, &producing)
            .await
        {
            log::error!(
                "failed to publish credit production for production unit {key}: {err}",
                key = production_unit.key(),
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
        domain::{
            Bloc, BlocKey, BlocName, Chance, LootFactors, SocialRule, SocialRuleFactorPerLevel, SocialRuleKey,
            SocialRuleLevel, SocialRuleName, Zone, ZoneKey, ZoneName, ZoneSocialRule,
        },
        services::credit_exchange_service::Cost,
    };

    fn point(x: f64, y: f64) -> Point {
        Point::new(NotNan::new(x).unwrap(), NotNan::new(y).unwrap())
    }

    fn distance(value: f64) -> Distance {
        serde_json::from_value(serde_json::json!(value)).unwrap()
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
        trust_with_social_rules(position, Vec::new())
    }

    fn trust_with_social_rules(position: Point, social_rules: Vec<ZoneSocialRule>) -> Trust {
        let bloc_name = BlocName::from("trust-bloc".to_string());
        let bloc_key = BlocKey::from("trust-bloc".to_string());
        let bloc = Arc::new(RwLock::new(Bloc::new(
            bloc_key.clone(),
            bloc_name.clone(),
            Chance::new(1),
            Share::default(),
        )));
        let zone = Arc::new(Zone::new_with_social_rules(
            ZoneKey::from("trust-zone-key".to_string()),
            ZoneName::from("trust-zone".to_string()),
            bloc_key,
            bloc_name,
            bloc,
            social_rules,
        ));
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
            bloc: BlocKey::from(bloc.to_string()),
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

    fn factor(
        trust: &Trust,
        units: &[UnitSnapshot],
        placements: &[PlacementSnapshot],
        world_bounds: WorldBounds,
        configured_radius: f64,
        close_units_factor: Share,
        combat_factor: Share,
    ) -> Share {
        let radius = applied_inhibition_radius(trust, placements, world_bounds, distance(configured_radius));
        inhibition_factor(trust, units, world_bounds, radius, close_units_factor, combat_factor)
    }

    #[test]
    fn production_is_not_inhibited_by_friendly_or_distant_units() {
        let trust = trust(point(10.0, 10.0));
        let units = [
            unit(point(10.5, 10.0), "trust-bloc"),
            unit(point(12.0, 10.0), "enemy-bloc"),
        ];

        let factor = factor(
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

        let factor = factor(
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

    #[tokio::test]
    async fn enemy_at_trust_uses_combat_factor_and_scales_resources() {
        let trust = trust(point(10.0, 10.0));
        let units = [
            unit(point(10.5, 10.0), "enemy-bloc"),
            unit(point(10.0, 10.0), "other-enemy-bloc"),
        ];

        let factor = factor(
            &trust,
            &units,
            &distant_placement(),
            world_bounds(),
            1.0,
            Share::from(0.75),
            Share::from(0.25),
        );
        let producing = trust.production_with_inhibition(factor).await;

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
        let applied_radius = applied_inhibition_radius(&trust, &placements, world_bounds(), distance(10.0));

        let outside_capped_radius = factor(
            &trust,
            &[unit(point(2.5, 10.0), "enemy-bloc")],
            &placements,
            world_bounds(),
            10.0,
            Share::from(0.75),
            Share::from(0.25),
        );
        let on_capped_radius = factor(
            &trust,
            &[unit(point(2.0, 10.0), "enemy-bloc")],
            &placements,
            world_bounds(),
            10.0,
            Share::from(0.75),
            Share::from(0.25),
        );

        assert_eq!(applied_radius, 1.0);
        assert_eq!(outside_capped_radius, Share::from(1.0));
        assert_eq!(on_capped_radius, Share::from(0.75));
    }

    #[tokio::test]
    async fn income_uses_inhibited_production_and_existing_resource_units() {
        let trust = trust(point(10.0, 10.0));
        let mut resource_totals = Resources::default();
        resource_totals.insert(ResourceName::new("iron".to_string()), 3.0);
        let mut context = ProductionContext {
            units: vec![],
            placements: vec![],
            world_bounds: world_bounds(),
            inhibition_radius: distance(1.0),
            close_units_factor: Share::from(0.75),
            combat_factor: Share::from(0.25),
            resource_totals,
        };

        let produced = trust.production_with_inhibition(Share::from(0.25)).await;
        assert_eq!(
            context.income_for(
                &trust,
                produced.into_iter().next().expect("trusts produce one resource")
            ),
            Money::from(1.0)
        );

        context.resource_totals = Resources::default();
        assert_eq!(
            context.income_for(
                &trust,
                produced.into_iter().next().expect("trusts produce one resource")
            ),
            Money::from(4.0)
        );
    }

    #[tokio::test]
    async fn social_rule_factor_is_multiplied_with_trust_inhibition() {
        let level = serde_json::from_value::<SocialRuleLevel>(serde_json::json!(2)).unwrap();
        let rule = SocialRule::new(
            SocialRuleKey::from("rule".to_string()),
            SocialRuleName::from("Rule".to_string()),
            serde_json::from_value(serde_json::json!(-2)).unwrap(),
            level,
            Some(serde_json::from_value::<SocialRuleFactorPerLevel>(serde_json::json!(0.1)).unwrap()),
            None,
        );
        let trust = trust_with_social_rules(
            point(10.0, 10.0),
            vec![ZoneSocialRule::new(rule, level)],
        );

        let produced = trust.production_with_inhibition(Share::from(0.25)).await;

        assert_eq!(
            produced.get(&ResourceName::new("iron".to_string())),
            Some(2.4)
        );
    }
}
