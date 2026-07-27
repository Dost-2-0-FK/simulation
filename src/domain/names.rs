use std::collections::HashMap;

use super::{BlocKey, BlocName, CharacterKey, CharacterName, ZoneKey, ZoneName};

#[derive(Debug, Clone)]
pub(crate) struct NameMappings {
    bloc_names: HashMap<BlocKey, BlocName>,
    bloc_keys: HashMap<BlocName, BlocKey>,
    zone_names: HashMap<ZoneKey, ZoneName>,
    zone_keys: HashMap<ZoneName, ZoneKey>,
    character_names: HashMap<CharacterKey, CharacterName>,
    character_keys: HashMap<CharacterName, CharacterKey>,
}

impl NameMappings {
    pub(crate) fn new(
        bloc_names: HashMap<BlocKey, BlocName>,
        zone_names: HashMap<ZoneKey, ZoneName>,
        character_names: HashMap<CharacterKey, CharacterName>,
    ) -> Self {
        let bloc_keys = bloc_names
            .iter()
            .map(|(key, name)| (name.clone(), key.clone()))
            .collect();
        let zone_keys = zone_names
            .iter()
            .map(|(key, name)| (name.clone(), key.clone()))
            .collect();
        let character_keys = character_names
            .iter()
            .map(|(key, name)| (name.clone(), key.clone()))
            .collect();

        Self {
            bloc_names,
            bloc_keys,
            zone_names,
            zone_keys,
            character_names,
            character_keys,
        }
    }

    pub(crate) fn bloc_name(&self, key: &BlocKey) -> Option<&BlocName> {
        self.bloc_names.get(key)
    }

    pub(crate) fn bloc_key(&self, name: &BlocName) -> Option<&BlocKey> {
        self.bloc_keys.get(name)
    }

    pub(crate) fn zone_name(&self, key: &ZoneKey) -> Option<&ZoneName> {
        self.zone_names.get(key)
    }

    #[expect(dead_code, reason = "reserved for frontend zone-name request parameters")]
    pub(crate) fn zone_key(&self, name: &ZoneName) -> Option<&ZoneKey> {
        self.zone_keys.get(name)
    }

    pub(crate) fn character_name(&self, key: &CharacterKey) -> Option<&CharacterName> {
        self.character_names.get(key)
    }

    pub(crate) fn character_key(&self, name: &CharacterName) -> Option<&CharacterKey> {
        self.character_keys.get(name)
    }
}
