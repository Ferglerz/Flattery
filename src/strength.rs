use pleasant_ui::math::bell_influence;
use serde::{Deserialize, Serialize};

pub const STRENGTH_REST_PX: f32 = 10.0;
pub const NODE_HIT_R: f32 = 16.0;
pub const MIN_NODE_Q: f64 = 3.0;
pub const MAX_NODE_Q: f64 = 18.0;
pub const DEFAULT_NODE_Q: f64 = 6.0;
const WEIGHT_MAX: f64 = 8.0;

pub fn q_to_norm(q: f64) -> f64 {
    let q = q.clamp(MIN_NODE_Q, MAX_NODE_Q);
    (q.ln() - MIN_NODE_Q.ln()) / (MAX_NODE_Q.ln() - MIN_NODE_Q.ln())
}

pub fn norm_to_q(norm: f64) -> f64 {
    let norm = norm.clamp(0.0, 1.0);
    (MIN_NODE_Q.ln() + norm * (MAX_NODE_Q.ln() - MIN_NODE_Q.ln())).exp()
}

pub fn q_to_width_pct(q: f64) -> f64 {
    (1.0 - q_to_norm(q)) * 100.0
}

pub fn width_pct_to_q(pct: f64) -> f64 {
    norm_to_q(1.0 - pct.clamp(0.0, 100.0) / 100.0)
}

pub fn default_node_q() -> f64 {
    DEFAULT_NODE_Q
}

pub fn default_node_radius() -> usize {
    1
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Polarity {
    Boost,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrengthNode {
    pub id: u64,
    pub freq: f64,
    /// 1.0 sits on the global rest line; 0.0 sits on 0 dB. May exceed 1.0 toward max boost/cut.
    pub weight: f64,
    #[serde(default = "default_node_q")]
    pub q: f64,
    #[serde(default = "default_node_radius")]
    pub radius: usize,
}

impl StrengthNode {
    pub fn new(id: u64, freq: f64, weight: f64) -> Self {
        let mut node = Self {
            id,
            freq,
            weight,
            q: DEFAULT_NODE_Q,
            radius: default_node_radius(),
        };
        node.sanitize();
        node
    }

    pub fn sanitize(&mut self) {
        if !self.freq.is_finite() {
            self.freq = 1000.0;
        }
        self.freq = self.freq.clamp(10.0, 22050.0);
        if !self.weight.is_finite() {
            self.weight = 1.0;
        }
        self.weight = self.weight.clamp(0.0, WEIGHT_MAX);
        if !self.q.is_finite() {
            self.q = DEFAULT_NODE_Q;
        }
        self.q = self.q.clamp(MIN_NODE_Q, MAX_NODE_Q);
        self.radius = self.radius.clamp(1, 12);
    }
}

pub fn next_node_id(nodes: &[StrengthNode]) -> u64 {
    nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1
}

/// Per-bin multiplier. Empty = flat 1.0 (global strength).
///
/// Each node is an independent bell, summed around the rest line so neighbors
/// do not pull a spline through each other.
pub fn weight_at(nodes: &[StrengthNode], freq: f64, _min_freq: f64, _max_freq: f64) -> f64 {
    if nodes.is_empty() {
        return 1.0;
    }
    let mut w = 1.0;
    for node in nodes {
        let inf = bell_influence(freq, node.freq, node.q);
        w += (node.weight - 1.0) * inf;
    }
    w.max(0.0)
}

pub fn fill_bin_weights(
    nodes: &[StrengthNode],
    bin_count: usize,
    bin_hz: f64,
    min_freq: f64,
    max_freq: f64,
    out: &mut [f64],
) {
    let n = bin_count.min(out.len());
    for (k, slot) in out.iter_mut().enumerate().take(n) {
        let freq = (k as f64 + 0.5) * bin_hz;
        *slot = weight_at(nodes, freq, min_freq, max_freq);
    }
}

/// Interpolate radius between nodes in log-frequency space using smoothstep.
pub fn radius_at(nodes: &[StrengthNode], freq: f64, default_radius: usize) -> f64 {
    if nodes.is_empty() {
        return default_radius as f64;
    }
    if nodes.len() == 1 {
        return nodes[0].radius as f64;
    }

    let mut sorted: Vec<&StrengthNode> = nodes.iter().collect();
    sorted.sort_by(|a, b| {
        a.freq
            .partial_cmp(&b.freq)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    if freq <= sorted[0].freq {
        return sorted[0].radius as f64;
    }
    if freq >= sorted.last().unwrap().freq {
        return sorted.last().unwrap().radius as f64;
    }

    for i in 0..sorted.len() - 1 {
        let n0 = sorted[i];
        let n1 = sorted[i + 1];
        if freq >= n0.freq && freq <= n1.freq {
            let f0_ln = n0.freq.ln();
            let f1_ln = n1.freq.ln();
            let t = if (f1_ln - f0_ln).abs() < 1e-6 {
                0.0
            } else {
                ((freq.ln() - f0_ln) / (f1_ln - f0_ln)).clamp(0.0, 1.0)
            };
            let smooth_t = t * t * (3.0 - 2.0 * t);
            return n0.radius as f64 + (n1.radius as f64 - n0.radius as f64) * smooth_t;
        }
    }

    sorted.last().unwrap().radius as f64
}

pub fn fill_bin_radii(
    nodes: &[StrengthNode],
    bin_count: usize,
    bin_hz: f64,
    default_radius: usize,
    out: &mut [usize],
) {
    let n = bin_count.min(out.len());
    for (k, slot) in out.iter_mut().enumerate().take(n) {
        let freq = (k as f64 + 0.5) * bin_hz;
        let r = radius_at(nodes, freq, default_radius).round();
        *slot = (r as usize).clamp(1, 12);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, freq: f64, weight: f64) -> StrengthNode {
        StrengthNode::new(id, freq, weight)
    }

    #[test]
    fn empty_curve_is_unity() {
        assert_eq!(weight_at(&[], 1000.0, 10.0, 22050.0), 1.0);
        assert_eq!(weight_at(&[], 80.0, 10.0, 22050.0), 1.0);
    }

    #[test]
    fn single_node_dips_at_center_and_holds_edges() {
        let nodes = [node(1, 1000.0, 0.0)];
        let mid = weight_at(&nodes, 1000.0, 10.0, 22050.0);
        let low = weight_at(&nodes, 20.0, 10.0, 22050.0);
        let high = weight_at(&nodes, 20000.0, 10.0, 22050.0);
        assert!(mid < 0.15, "expected a dip at the node, got {mid}");
        assert!(low > 0.85, "low edge should stay near 1, got {low}");
        assert!(high > 0.85, "high edge should stay near 1, got {high}");
    }

    #[test]
    fn distant_nodes_do_not_tie_the_midband() {
        let nodes = [node(1, 80.0, 0.0), node(2, 12000.0, 0.0)];
        let mid = weight_at(&nodes, 1000.0, 10.0, 22050.0);
        assert!(mid > 0.7, "bells should not drag 1 kHz toward 0, got {mid}");
    }

    #[test]
    fn weights_are_clamped() {
        let nodes = [node(1, 1000.0, 40.0)];
        let w = weight_at(&nodes, 1000.0, 10.0, 22050.0);
        assert!(w <= WEIGHT_MAX + 1e-9);
    }

    #[test]
    fn fill_bin_weights_matches_weight_at() {
        let nodes = [node(1, 1800.0, 0.25)];
        let bin_hz = 44100.0 / 512.0;
        let mut out = vec![0.0; 256];
        fill_bin_weights(&nodes, 256, bin_hz, 10.0, 22050.0, &mut out);
        let k = (1800.0 / bin_hz).floor() as usize;
        let expected = weight_at(&nodes, (k as f64 + 0.5) * bin_hz, 10.0, 22050.0);
        assert!((out[k] - expected).abs() < 1e-9);
    }
}
