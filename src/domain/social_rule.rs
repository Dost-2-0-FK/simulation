use serde::{Deserialize, Serialize};

use crate::domain::ProductionFactor;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct SocialRuleKey(String);

impl From<String> for SocialRuleKey {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl SocialRuleKey {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash, derive_more::Display, utoipa::ToSchema)]
pub(crate) struct SocialRuleName(String);

impl From<String> for SocialRuleName {
    fn from(value: String) -> Self {
        Self(value)
    }
}

#[derive(
    Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, derive_more::Display, utoipa::ToSchema,
)]
pub(crate) struct SocialRuleLevel(i32);

impl SocialRuleLevel {
    pub(crate) fn value(self) -> i32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, utoipa::ToSchema)]
pub(crate) struct SocialRuleFactorPerLevel(f32);

impl SocialRuleFactorPerLevel {
    pub(crate) fn value(self) -> f32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for SocialRuleFactorPerLevel {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f32::deserialize(deserializer)?;
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(serde::de::Error::custom(
                "social-rule factor per level must be finite",
            ))
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SocialRule {
    key: SocialRuleKey,
    name: SocialRuleName,
    min_level: SocialRuleLevel,
    max_level: SocialRuleLevel,
    trust_production_factor_per_level: Option<SocialRuleFactorPerLevel>,
    production_unit_factor_per_level: Option<SocialRuleFactorPerLevel>,
}

impl SocialRule {
    pub(crate) fn new(
        key: SocialRuleKey,
        name: SocialRuleName,
        min_level: SocialRuleLevel,
        max_level: SocialRuleLevel,
        trust_production_factor_per_level: Option<SocialRuleFactorPerLevel>,
        production_unit_factor_per_level: Option<SocialRuleFactorPerLevel>,
    ) -> Self {
        Self {
            key,
            name,
            min_level,
            max_level,
            trust_production_factor_per_level,
            production_unit_factor_per_level,
        }
    }

    pub(crate) fn key(&self) -> &SocialRuleKey {
        &self.key
    }

    pub(crate) fn name(&self) -> &SocialRuleName {
        &self.name
    }

    pub(crate) fn min_level(&self) -> SocialRuleLevel {
        self.min_level
    }

    pub(crate) fn max_level(&self) -> SocialRuleLevel {
        self.max_level
    }

    pub(crate) fn trust_production_factor_per_level(&self) -> Option<SocialRuleFactorPerLevel> {
        self.trust_production_factor_per_level
    }

    pub(crate) fn production_unit_factor_per_level(&self) -> Option<SocialRuleFactorPerLevel> {
        self.production_unit_factor_per_level
    }

    pub(crate) fn accepts(&self, level: SocialRuleLevel) -> bool {
        (self.min_level..=self.max_level).contains(&level)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ZoneSocialRule {
    rule: SocialRule,
    level: SocialRuleLevel,
}

impl ZoneSocialRule {
    pub(crate) fn new(rule: SocialRule, level: SocialRuleLevel) -> Self {
        Self { rule, level }
    }

    pub(crate) fn rule(&self) -> &SocialRule {
        &self.rule
    }

    pub(crate) fn level(&self) -> SocialRuleLevel {
        self.level
    }

    pub(crate) fn set_level(&mut self, level: SocialRuleLevel) {
        self.level = level;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SocialRuleLevelChange {
    key: SocialRuleKey,
    name: SocialRuleName,
    old_level: SocialRuleLevel,
    level: SocialRuleLevel,
}

impl SocialRuleLevelChange {
    pub(crate) fn new(
        key: SocialRuleKey,
        name: SocialRuleName,
        old_level: SocialRuleLevel,
        level: SocialRuleLevel,
    ) -> Self {
        Self {
            key,
            name,
            old_level,
            level,
        }
    }

    pub(crate) fn key(&self) -> &SocialRuleKey {
        &self.key
    }

    pub(crate) fn name(&self) -> &SocialRuleName {
        &self.name
    }

    pub(crate) fn old_level(&self) -> SocialRuleLevel {
        self.old_level
    }

    pub(crate) fn level(&self) -> SocialRuleLevel {
        self.level
    }
}

pub(crate) fn production_factor(
    social_rules: &[ZoneSocialRule],
    factor_per_level: impl Fn(&SocialRule) -> Option<SocialRuleFactorPerLevel>,
) -> ProductionFactor {
    let contribution = social_rules
        .iter()
        .filter_map(|assignment| {
            factor_per_level(assignment.rule())
                .map(|factor| assignment.level().value() as f32 * factor.value())
        })
        .sum::<f32>();
    ProductionFactor::new(1.0 + contribution)
        .expect("configured social-rule ranges guarantee non-negative production factors")
}
