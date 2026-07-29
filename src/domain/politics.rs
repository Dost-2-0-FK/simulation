use std::{
    collections::HashSet,
    sync::Arc,
};

use derive_more::Display;
use rand::RngExt as _;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, RwLockReadGuard};

use crate::{
    domain::{
        ProductionFactor, SocialRuleKey, SocialRuleLevel, SocialRuleLevelChange, SocialRuleName, ZoneSocialRule,
        social_rule::production_factor,
    },
    services::credit_exchange_service::Share,
};

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ZoneName(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct ZoneKey(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct BlocKey(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct CharacterKey(String);

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct CharacterName(String);

macro_rules! string_identifier {
    ($type:ty) => {
        impl From<String> for $type {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<$type> for String {
            fn from(value: $type) -> Self {
                value.0
            }
        }

    };
}

string_identifier!(ZoneName);
string_identifier!(ZoneKey);
string_identifier!(BlocName);
string_identifier!(BlocKey);
string_identifier!(CharacterKey);
string_identifier!(CharacterName);

impl BlocKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl ZoneKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl CharacterKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug)]
pub(crate) struct Zone {
    key: ZoneKey,
    name: ZoneName,
    bloc_key: BlocKey,
    bloc_name: BlocName,
    bloc: Arc<RwLock<Bloc>>,
    social_rules: RwLock<Vec<ZoneSocialRule>>,
}

impl Zone {
    pub(crate) fn new_with_social_rules(
        key: ZoneKey,
        name: ZoneName,
        bloc_key: BlocKey,
        bloc_name: BlocName,
        bloc: Arc<RwLock<Bloc>>,
        social_rules: Vec<ZoneSocialRule>,
    ) -> Self {
        Self {
            key,
            name,
            bloc_key,
            bloc_name,
            bloc,
            social_rules: RwLock::new(social_rules),
        }
    }

    pub(crate) fn key(&self) -> &ZoneKey {
        &self.key
    }

    pub(crate) fn name(&self) -> &ZoneName {
        &self.name
    }

    pub(crate) fn bloc_name(&self) -> &BlocName {
        &self.bloc_name
    }

    pub(crate) fn bloc_key(&self) -> &BlocKey {
        &self.bloc_key
    }

    pub(crate) async fn bloc(&self) -> RwLockReadGuard<'_, Bloc> {
        self.bloc.read().await
    }

    pub(crate) async fn social_rules(&self) -> Vec<ZoneSocialRule> {
        self.social_rules.read().await.clone()
    }

    pub(crate) async fn trust_production_factor(&self) -> ProductionFactor {
        production_factor(
            &self.social_rules.read().await,
            |rule| rule.trust_production_factor_per_level(),
        )
    }

    pub(crate) async fn production_unit_factor(&self) -> ProductionFactor {
        production_factor(
            &self.social_rules.read().await,
            |rule| rule.production_unit_factor_per_level(),
        )
    }

    pub(crate) async fn patch_social_rule_levels(
        &self,
        patches: &[(SocialRuleName, SocialRuleLevel)],
    ) -> core::result::Result<(), SocialRulePatchError> {
        let mut social_rules = self.social_rules.write().await;
        validate_social_rule_level_patch(&social_rules, patches)?;

        for (name, level) in patches {
            social_rules
                .iter_mut()
                .find(|assignment| assignment.rule().name() == name)
                .expect("social-rule patches were validated above")
                .set_level(*level);
        }
        Ok(())
    }

    pub(crate) async fn social_rule_level_changes(
        &self,
        patches: &[(SocialRuleName, SocialRuleLevel)],
    ) -> core::result::Result<Vec<SocialRuleLevelChange>, SocialRulePatchError> {
        let social_rules = self.social_rules.read().await;
        validate_social_rule_level_patch(&social_rules, patches)?;

        Ok(patches
            .iter()
            .filter_map(|(name, level)| {
                let assignment = social_rules
                    .iter()
                    .find(|assignment| assignment.rule().name() == name)
                    .expect("social-rule patches were validated above");
                (assignment.level() != *level).then(|| {
                    SocialRuleLevelChange::new(
                        assignment.rule().key().clone(),
                        name.clone(),
                        assignment.level(),
                        *level,
                    )
                })
            })
            .collect())
    }

    pub(crate) async fn apply_persisted_social_rule_level(
        &self,
        key: &SocialRuleKey,
        level: SocialRuleLevel,
    ) -> core::result::Result<(), PersistedSocialRuleError> {
        let mut social_rules = self.social_rules.write().await;
        let assignment = social_rules
            .iter_mut()
            .find(|assignment| assignment.rule().key() == key)
            .ok_or(PersistedSocialRuleError::UnassignedRule)?;
        if !assignment.rule().accepts(level) {
            return Err(PersistedSocialRuleError::LevelOutOfRange {
                min: assignment.rule().min_level(),
                max: assignment.rule().max_level(),
            });
        }
        assignment.set_level(level);
        Ok(())
    }
}

fn validate_social_rule_level_patch(
    social_rules: &[ZoneSocialRule],
    patches: &[(SocialRuleName, SocialRuleLevel)],
) -> core::result::Result<(), SocialRulePatchError> {
    let mut names = HashSet::with_capacity(patches.len());
    for (name, _) in patches {
        if !names.insert(name) {
            return Err(SocialRulePatchError::DuplicateRule(name.clone()));
        }
    }

    for (name, level) in patches {
        let assignment = social_rules
            .iter()
            .find(|assignment| assignment.rule().name() == name)
            .ok_or_else(|| SocialRulePatchError::UnassignedRule(name.clone()))?;
        if !assignment.rule().accepts(*level) {
            return Err(SocialRulePatchError::LevelOutOfRange {
                name: name.clone(),
                level: *level,
                min: assignment.rule().min_level(),
                max: assignment.rule().max_level(),
            });
        }
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SocialRulePatchError {
    DuplicateRule(SocialRuleName),
    UnassignedRule(SocialRuleName),
    LevelOutOfRange {
        name: SocialRuleName,
        level: SocialRuleLevel,
        min: SocialRuleLevel,
        max: SocialRuleLevel,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PersistedSocialRuleError {
    UnassignedRule,
    LevelOutOfRange {
        min: SocialRuleLevel,
        max: SocialRuleLevel,
    },
}

/// Every unit belongs to a [Zone], which belongs to a [Bloc], which implies a [Chance].
/// When two units of a different [Bloc] meet, they fight: For each unit, a die is rolled, i.e., a  uniform random draw
/// of [0, [Chance]]. On 0, the other unit is eliminated. If both dice show 0, both units are eliminated.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, utoipa::ToSchema)]
pub(crate) struct Chance(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DieRollOutcome {
    Hit,
    Miss,
}

impl Chance {
    pub(crate) fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) fn roll(&self) -> DieRollOutcome {
        if rand::rng().random_range(0..=self.0 - 1) == 0 {
            return DieRollOutcome::Hit;
        }
        DieRollOutcome::Miss
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, Display, utoipa::ToSchema)]
pub(crate) struct BlocName(String);

#[derive(Debug, Clone)]
pub(crate) struct Bloc {
    key: BlocKey,
    name: BlocName,
    chance: Chance,
    military_expense: Share,
}

impl Bloc {
    pub(crate) fn new(key: BlocKey, name: BlocName, chance: Chance, military_expense: Share) -> Self {
        Self {
            key,
            name,
            chance,
            military_expense,
        }
    }

    pub(crate) fn name(&self) -> &BlocName {
        &self.name
    }

    pub(crate) fn key(&self) -> &BlocKey {
        &self.key
    }

    pub(crate) fn chance(&self) -> Chance {
        self.chance
    }

    pub(crate) fn military_expense(&self) -> Share {
        self.military_expense
    }

    #[expect(dead_code)]
    pub(crate) fn set_chance(&mut self, chance: Chance) {
        self.chance = chance;
    }

    #[expect(dead_code)]
    pub(crate) fn set_military_expense(&mut self, military_expense: Share) {
        self.military_expense = military_expense;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{SocialRule, SocialRuleFactorPerLevel, ZoneSocialRule};

    fn level(value: i32) -> SocialRuleLevel {
        serde_json::from_value(value.into()).unwrap()
    }

    fn factor(value: f32) -> SocialRuleFactorPerLevel {
        serde_json::from_value(value.into()).unwrap()
    }

    fn rule(key: &str, name: &str, trust_factor: f32, production_unit_factor: f32) -> SocialRule {
        SocialRule::new(
            SocialRuleKey::from(key.to_string()),
            SocialRuleName::from(name.to_string()),
            level(-2),
            level(2),
            Some(factor(trust_factor)),
            Some(factor(production_unit_factor)),
        )
    }

    fn zone() -> Zone {
        let bloc_key = BlocKey::from("bloc".to_string());
        let bloc_name = BlocName::from("Bloc".to_string());
        Zone::new_with_social_rules(
            ZoneKey::from("zone".to_string()),
            ZoneName::from("Zone".to_string()),
            bloc_key.clone(),
            bloc_name.clone(),
            Arc::new(RwLock::new(Bloc::new(
                bloc_key,
                bloc_name,
                Chance::new(1),
                Share::default(),
            ))),
            vec![
                ZoneSocialRule::new(rule("one", "One", 0.1, 0.2), level(2)),
                ZoneSocialRule::new(rule("two", "Two", -0.2, -0.1), level(-1)),
            ],
        )
    }

    #[tokio::test]
    async fn social_rule_contributions_are_added_into_one_factor() {
        let zone = zone();

        assert_eq!(zone.trust_production_factor().await.value(), 1.4);
        assert_eq!(zone.production_unit_factor().await.value(), 1.5);
    }

    #[tokio::test]
    async fn social_rule_patch_is_atomic() {
        let zone = zone();
        let result = zone.patch_social_rule_levels(&[
            (SocialRuleName::from("One".to_string()), level(0)),
            (SocialRuleName::from("Two".to_string()), level(3)),
        ])
        .await;

        assert!(matches!(result, Err(SocialRulePatchError::LevelOutOfRange { .. })));
        assert_eq!(
            zone.social_rules()
                .await
                .iter()
                .map(ZoneSocialRule::level)
                .collect::<Vec<_>>(),
            vec![level(2), level(-1)]
        );
    }

    #[tokio::test]
    async fn social_rule_patch_rejects_duplicate_and_unassigned_names() {
        let zone = zone();
        let duplicate = SocialRuleName::from("One".to_string());
        assert_eq!(
            zone.patch_social_rule_levels(&[(duplicate.clone(), level(0)), (duplicate.clone(), level(1))])
                .await,
            Err(SocialRulePatchError::DuplicateRule(duplicate))
        );

        let missing = SocialRuleName::from("Missing".to_string());
        assert_eq!(
            zone.patch_social_rule_levels(&[(missing.clone(), level(0))]).await,
            Err(SocialRulePatchError::UnassignedRule(missing))
        );
    }

    #[tokio::test]
    async fn social_rule_level_changes_contain_only_genuine_changes_in_patch_order() {
        let zone = zone();

        let changes = zone
            .social_rule_level_changes(&[
                (SocialRuleName::from("Two".to_string()), level(0)),
                (SocialRuleName::from("One".to_string()), level(2)),
            ])
            .await
            .unwrap();

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key(), &SocialRuleKey::from("two".to_string()));
        assert_eq!(changes[0].name(), &SocialRuleName::from("Two".to_string()));
        assert_eq!(changes[0].old_level(), level(-1));
        assert_eq!(changes[0].level(), level(0));
    }
}
