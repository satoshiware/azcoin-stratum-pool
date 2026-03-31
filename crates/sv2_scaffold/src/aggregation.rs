use std::collections::HashMap;

/// Pool-oriented miner identity. Tracks username and worker separately.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MinerId {
    pub username: String,
    pub worker: String,
}

/// In-memory share accounting record for pooled mining.
#[derive(Debug, Clone)]
pub struct ShareRecord {
    pub miner: MinerId,
    pub difficulty: f64,
    pub timestamp: u64,
    pub accepted: bool,
}

/// Minimal pooled-miner rejection markers for per-miner diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectionReason {
    Duplicate,
    Stale,
    LowDifficulty,
    Other(String),
}

/// Per-miner event types kept alongside pooled accounting stats.
#[derive(Debug, Clone, PartialEq)]
pub enum MinerEventKind {
    Registered,
    Authorized,
    DifficultyAssigned { difficulty: f64 },
    JobObserved { job_id: String },
    ShareSubmitted { difficulty: f64 },
    ShareAccepted { difficulty: f64 },
    ShareRejected { reason: RejectionReason },
}

/// In-memory diagnostic event retained per pooled miner.
#[derive(Debug, Clone, PartialEq)]
pub struct MinerEvent {
    pub timestamp: u64,
    pub kind: MinerEventKind,
}

/// Minimal pooled-mining stats used for accounting inputs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MinerStats {
    pub total_shares: u64,
    pub accepted_shares: u64,
    pub rejected_shares: u64,
    pub last_share_time: u64,
}

/// Minimal in-memory aggregation engine for pooled mining.
pub struct AggregationEngine {
    miners: HashMap<MinerId, MinerStats>,
    events: HashMap<MinerId, Vec<MinerEvent>>,
}

impl AggregationEngine {
    pub fn new() -> Self {
        Self {
            miners: HashMap::new(),
            events: HashMap::new(),
        }
    }

    pub fn register_miner(&mut self, miner: MinerId) {
        self.miners.entry(miner.clone()).or_default();
        self.events.entry(miner).or_default();
    }

    pub fn record_event(&mut self, miner: MinerId, event: MinerEvent) {
        self.miners.entry(miner.clone()).or_default();
        self.events.entry(miner).or_default().push(event);
    }

    pub fn record_share(&mut self, record: ShareRecord) {
        self.events.entry(record.miner.clone()).or_default();
        let stats = self.miners.entry(record.miner).or_default();
        stats.total_shares += 1;
        stats.last_share_time = record.timestamp;
        if record.accepted {
            stats.accepted_shares += 1;
        } else {
            stats.rejected_shares += 1;
        }
    }

    pub fn get_stats(&self, miner: &MinerId) -> Option<&MinerStats> {
        self.miners.get(miner)
    }

    pub fn get_events(&self, miner: &MinerId) -> Option<&[MinerEvent]> {
        self.events.get(miner).map(Vec::as_slice)
    }
}

impl Default for AggregationEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn miner(username: &str, worker: &str) -> MinerId {
        MinerId {
            username: username.to_string(),
            worker: worker.to_string(),
        }
    }

    #[test]
    fn test_record_share_tracks_pool_stats_per_miner() {
        let mut engine = AggregationEngine::new();
        let miner_a = miner("alice", "rig1");
        let miner_b = miner("bob", "rig2");

        engine.register_miner(miner_a.clone());
        engine.record_share(ShareRecord {
            miner: miner_a.clone(),
            difficulty: 32.0,
            timestamp: 100,
            accepted: true,
        });
        engine.record_share(ShareRecord {
            miner: miner_a.clone(),
            difficulty: 32.0,
            timestamp: 105,
            accepted: false,
        });
        engine.record_share(ShareRecord {
            miner: miner_b.clone(),
            difficulty: 64.0,
            timestamp: 200,
            accepted: true,
        });

        let stats_a = engine.get_stats(&miner_a).unwrap();
        assert_eq!(stats_a.total_shares, 2);
        assert_eq!(stats_a.accepted_shares, 1);
        assert_eq!(stats_a.rejected_shares, 1);
        assert_eq!(stats_a.last_share_time, 105);

        let stats_b = engine.get_stats(&miner_b).unwrap();
        assert_eq!(stats_b.total_shares, 1);
        assert_eq!(stats_b.accepted_shares, 1);
        assert_eq!(stats_b.rejected_shares, 0);
        assert_eq!(stats_b.last_share_time, 200);
    }

    #[test]
    fn test_events_are_tracked_per_miner_without_affecting_pool_stats() {
        let mut engine = AggregationEngine::new();
        let miner_a = miner("alice", "rig1");
        let miner_b = miner("bob", "rig2");

        engine.record_event(
            miner_a.clone(),
            MinerEvent {
                timestamp: 10,
                kind: MinerEventKind::Registered,
            },
        );
        engine.record_event(
            miner_a.clone(),
            MinerEvent {
                timestamp: 11,
                kind: MinerEventKind::DifficultyAssigned { difficulty: 64.0 },
            },
        );
        engine.record_event(
            miner_b.clone(),
            MinerEvent {
                timestamp: 20,
                kind: MinerEventKind::Authorized,
            },
        );
        engine.record_event(
            miner_b.clone(),
            MinerEvent {
                timestamp: 21,
                kind: MinerEventKind::ShareRejected {
                    reason: RejectionReason::LowDifficulty,
                },
            },
        );

        engine.record_share(ShareRecord {
            miner: miner_a.clone(),
            difficulty: 64.0,
            timestamp: 12,
            accepted: true,
        });
        engine.record_share(ShareRecord {
            miner: miner_b.clone(),
            difficulty: 64.0,
            timestamp: 22,
            accepted: false,
        });

        let events_a = engine.get_events(&miner_a).unwrap();
        assert_eq!(events_a.len(), 2);
        assert_eq!(events_a[0].kind, MinerEventKind::Registered);
        assert_eq!(
            events_a[1].kind,
            MinerEventKind::DifficultyAssigned { difficulty: 64.0 }
        );

        let events_b = engine.get_events(&miner_b).unwrap();
        assert_eq!(events_b.len(), 2);
        assert_eq!(events_b[0].kind, MinerEventKind::Authorized);
        assert_eq!(
            events_b[1].kind,
            MinerEventKind::ShareRejected {
                reason: RejectionReason::LowDifficulty
            }
        );

        let stats_a = engine.get_stats(&miner_a).unwrap();
        assert_eq!(stats_a.total_shares, 1);
        assert_eq!(stats_a.accepted_shares, 1);
        assert_eq!(stats_a.rejected_shares, 0);

        let stats_b = engine.get_stats(&miner_b).unwrap();
        assert_eq!(stats_b.total_shares, 1);
        assert_eq!(stats_b.accepted_shares, 0);
        assert_eq!(stats_b.rejected_shares, 1);
    }
}
