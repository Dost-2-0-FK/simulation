use serde::{Deserialize, Serialize};

use crate::domain::SimulationStats;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(super) struct PersistedStats {
    #[serde(rename = "_id")]
    id: u8,
    stats: SimulationStats,
}

impl PersistedStats {
    pub(super) fn new(stats: SimulationStats) -> Self {
        Self { id: 0, stats }
    }

    pub(super) fn into_stats(self) -> SimulationStats {
        self.stats
    }
}
