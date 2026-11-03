use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BondingPolicy {
    Smart,
    MaxSpeed,
    Balanced,
    Failover,
    Custom,
}

impl Default for BondingPolicy {
    fn default() -> Self {
        Self::Smart
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubflowDescriptor {
    pub subflow_id: u32,
    pub throughput_bps: u64,
    pub link_capacity_bps: u64,
    pub rtt_ms: f64,
    pub loss_pct: f64,
    pub is_primary: bool,
    pub custom_weight: u32,
    pub alive: bool,
    pub inflight_bytes: u64,
}

/// Computes normalized percentage weights (summing to 100.0) across alive subflows.
pub fn compute_subflow_weights(
    policy: BondingPolicy,
    subflows: &[SubflowDescriptor],
) -> Vec<(u32, f64)> {
    let alive: Vec<&SubflowDescriptor> = subflows.iter().filter(|s| s.alive).collect();
    if alive.is_empty() {
        return subflows.iter().map(|s| (s.subflow_id, 0.0)).collect();
    }

    let mut raw: HashMap<u32, f64> = HashMap::new();

    match policy {
        BondingPolicy::Failover => {
            // Pick primary if healthy, else healthiest backup
            let chosen = alive
                .iter()
                .find(|s| s.is_primary && s.loss_pct < 25.0)
                .copied()
                .or_else(|| {
                    alive.iter().copied().max_by(|a, b| {
                        let score_a = effective_capacity(a) / (1.0 + a.loss_pct * 5.0);
                        let score_b = effective_capacity(b) / (1.0 + b.loss_pct * 5.0);
                        score_a
                            .partial_cmp(&score_b)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
                });

            if let Some(winner) = chosen {
                raw.insert(winner.subflow_id, 100.0);
            }
        }
        BondingPolicy::Custom => {
            for s in &alive {
                raw.insert(s.subflow_id, (s.custom_weight.max(1)) as f64);
            }
        }
        BondingPolicy::MaxSpeed => {
            let max_cap = alive
                .iter()
                .map(|s| effective_capacity(s))
                .fold(1.0_f64, f64::max);
            for s in &alive {
                let cap = effective_capacity(s);
                // Prevent a severely degraded slow link (<3% of fastest & high RTT) from stalling tail chunks
                if alive.len() > 1 && cap < max_cap * 0.03 && s.rtt_ms > 280.0 {
                    raw.insert(s.subflow_id, max_cap * 0.01);
                } else {
                    let loss_factor = (1.0 - (s.loss_pct / 60.0).min(0.90)).max(0.05);
                    raw.insert(s.subflow_id, cap * loss_factor);
                }
            }
        }
        BondingPolicy::Balanced => {
            for s in &alive {
                let cap_mbps = (effective_capacity(s) / 1_000_000.0).max(0.5);
                let rtt = s.rtt_ms.max(8.0);
                let loss_penalty = (1.0 - (s.loss_pct / 30.0).min(0.95)).powi(2);
                let score = (cap_mbps.sqrt() * 100.0 / rtt) * loss_penalty;
                raw.insert(s.subflow_id, score.max(0.1));
            }
        }
        BondingPolicy::Smart => {
            for s in &alive {
                let cap_mbps = (effective_capacity(s) / 1_000_000.0).max(0.5);
                let rtt_factor = 1.0 / (s.rtt_ms.max(8.0)).sqrt();
                let loss_factor = (1.0 - (s.loss_pct / 35.0).min(0.95)).powi(2);
                // Penalize links with excessive unacked queue buildup
                let congestion_factor = 1.0 / (1.0 + (s.inflight_bytes as f64 / (512.0 * 1024.0)));
                let score = cap_mbps * rtt_factor * loss_factor * congestion_factor;
                raw.insert(s.subflow_id, score.max(0.05));
            }
        }
    }

    let sum: f64 = raw.values().sum();
    subflows
        .iter()
        .map(|s| {
            let pct = if sum > 0.0 {
                (raw.get(&s.subflow_id).copied().unwrap_or(0.0) / sum) * 100.0
            } else {
                0.0
            };
            (s.subflow_id, pct)
        })
        .collect()
}

fn effective_capacity(s: &SubflowDescriptor) -> f64 {
    if s.throughput_bps > 256_000 {
        // Blend observed throughput with link speed hint
        (s.throughput_bps as f64) * 0.85 + (s.link_capacity_bps.min(1_000_000_000) as f64) * 0.15
    } else if s.link_capacity_bps > 0 {
        (s.link_capacity_bps.min(500_000_000)) as f64 * 0.25
    } else {
        10_000_000.0
    }
}

/// Weighted Deficit Round-Robin Multipath Packet/Chunk Scheduler.
#[derive(Debug, Default)]
pub struct MultipathScheduler {
    deficits: HashMap<u32, f64>,
}

impl MultipathScheduler {
    pub fn new() -> Self {
        Self {
            deficits: HashMap::new(),
        }
    }

    /// Selects the optimal alive `subflow_id` for a chunk of `chunk_bytes`.
    pub fn select_subflow(
        &mut self,
        policy: BondingPolicy,
        subflows: &[SubflowDescriptor],
        chunk_bytes: usize,
    ) -> Option<u32> {
        let weights = compute_subflow_weights(policy, subflows);
        let mut best_id: Option<u32> = None;
        let mut best_credit = f64::MIN;

        for (id, pct) in &weights {
            if *pct <= 0.0 {
                continue;
            }
            let entry = self.deficits.entry(*id).or_insert(0.0);
            *entry += *pct * (chunk_bytes.max(1024) as f64 / 100.0);
            if *entry > best_credit {
                best_credit = *entry;
                best_id = Some(*id);
            }
        }

        if let Some(chosen) = best_id {
            if let Some(d) = self.deficits.get_mut(&chosen) {
                *d -= chunk_bytes.max(1024) as f64;
            }
        }
        best_id
    }
}

/// Deterministic sliding-window byte-offset reassembler for multipath streams.
/// Handles out-of-order, duplicate, or retransmitted chunks arriving across heterogeneous links.
#[derive(Debug)]
pub struct StreamReassembler {
    next_offset: u64,
    pending: BTreeMap<u64, Vec<u8>>,
    eof_offset: Option<u64>,
    buffered_bytes: usize,
    max_buffer_bytes: usize,
}

impl StreamReassembler {
    pub fn new(max_buffer_bytes: usize) -> Self {
        Self {
            next_offset: 0,
            pending: BTreeMap::new(),
            eof_offset: None,
            buffered_bytes: 0,
            max_buffer_bytes,
        }
    }

    pub fn next_expected_offset(&self) -> u64 {
        self.next_offset
    }

    pub fn queued_chunks(&self) -> usize {
        self.pending.len()
    }

    pub fn is_complete(&self) -> bool {
        if let Some(eof) = self.eof_offset {
            self.next_offset >= eof && self.pending.is_empty()
        } else {
            false
        }
    }

    /// Inserts a chunk at `offset` and returns all newly contiguous ordered bytes ready for delivery.
    pub fn push_chunk(
        &mut self,
        offset: u64,
        mut data: Vec<u8>,
        eof: bool,
    ) -> Result<Vec<u8>, String> {
        let chunk_end = offset + data.len() as u64;
        if eof {
            self.eof_offset = Some(chunk_end);
        }

        // Trim already-delivered prefix if duplicate or overlapping retransmission
        if offset < self.next_offset {
            if chunk_end <= self.next_offset {
                return Ok(Vec::new());
            }
            let skip = (self.next_offset - offset) as usize;
            data.drain(0..skip);
            return self.push_chunk(self.next_offset, data, eof);
        }

        if !data.is_empty() {
            if offset > self.next_offset
                && self.buffered_bytes + data.len() > self.max_buffer_bytes
            {
                return Err("Multipath reassembly window exceeded maximum memory bound".into());
            }
            if let Some(existing) = self.pending.get(&offset) {
                if existing.len() >= data.len() {
                    return Ok(self.drain_ready());
                }
                self.buffered_bytes = self.buffered_bytes.saturating_sub(existing.len());
            }
            self.buffered_bytes += data.len();
            self.pending.insert(offset, data);
        }

        Ok(self.drain_ready())
    }

    fn drain_ready(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        while let Some((&first_offset, _)) = self.pending.first_key_value() {
            if first_offset > self.next_offset {
                break;
            }
            let chunk = self.pending.remove(&first_offset).unwrap();
            self.buffered_bytes = self.buffered_bytes.saturating_sub(chunk.len());
            let chunk_end = first_offset + chunk.len() as u64;
            if chunk_end > self.next_offset {
                let skip = (self.next_offset - first_offset) as usize;
                out.extend_from_slice(&chunk[skip..]);
                self.next_offset = chunk_end;
            }
        }
        out
    }
}

