use serde::{Deserialize, Serialize};

pub const STRENGTH_REST_PX: f32 = 10.0;
pub const NODE_HIT_R: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Polarity {
    Boost,
    Cut,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrengthNode {
    pub id: u64,
    pub freq: f64,
    /// 1.0 sits on the global offset line; 0.0 sits on the 0 dB center (no effect).
    pub weight: f64,
}

impl StrengthNode {
    pub fn sanitize(&mut self) {
        if !self.freq.is_finite() {
            self.freq = 1000.0;
        }
        self.freq = self.freq.clamp(10.0, 22050.0);
        if !self.weight.is_finite() {
            self.weight = 1.0;
        }
        self.weight = self.weight.clamp(0.0, 1.0);
    }
}

pub fn next_node_id(nodes: &[StrengthNode]) -> u64 {
    nodes.iter().map(|n| n.id).max().unwrap_or(0) + 1
}

/// Per-bin multiplier in 0..=1.
///
/// No nodes means a flat 1.0 (the global strength param applies uniformly).
/// Nodes are dips/peaks relative to that line, with implicit endpoints held at 1.0.
pub fn weight_at(nodes: &[StrengthNode], freq: f64, min_freq: f64, max_freq: f64) -> f64 {
    if nodes.is_empty() {
        return 1.0;
    }

    let knots = build_knots(nodes, min_freq, max_freq);
    if knots.len() == 1 {
        return knots[0].1.clamp(0.0, 1.0);
    }

    let x = log_freq(freq.clamp(min_freq, max_freq));
    if x <= knots[0].0 {
        return knots[0].1.clamp(0.0, 1.0);
    }
    let last = knots.len() - 1;
    if x >= knots[last].0 {
        return knots[last].1.clamp(0.0, 1.0);
    }

    for i in 0..last {
        if x <= knots[i + 1].0 {
            let span = (knots[i + 1].0 - knots[i].0).max(1e-9);
            let t = ((x - knots[i].0) / span).clamp(0.0, 1.0);
            let p0 = if i == 0 { knots[i].1 } else { knots[i - 1].1 };
            let p1 = knots[i].1;
            let p2 = knots[i + 1].1;
            let p3 = if i + 2 <= last {
                knots[i + 2].1
            } else {
                knots[i + 1].1
            };
            return catmull(p0, p1, p2, p3, t).clamp(0.0, 1.0);
        }
    }

    knots[last].1.clamp(0.0, 1.0)
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

fn log_freq(freq: f64) -> f64 {
    freq.max(1.0).ln()
}

fn build_knots(nodes: &[StrengthNode], min_freq: f64, max_freq: f64) -> Vec<(f64, f64)> {
    let mut nodes: Vec<&StrengthNode> = nodes.iter().collect();
    nodes.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap_or(std::cmp::Ordering::Equal));

    let mut knots = Vec::with_capacity(nodes.len() + 2);
    knots.push((log_freq(min_freq), 1.0));
    for node in nodes {
        let x = log_freq(node.freq.clamp(min_freq, max_freq));
        if (x - knots.last().unwrap().0).abs() < 1e-4 {
            *knots.last_mut().unwrap() = (x, node.weight);
        } else {
            knots.push((x, node.weight));
        }
    }
    let max_x = log_freq(max_freq);
    if (max_x - knots.last().unwrap().0).abs() < 1e-4 {
        knots.last_mut().unwrap().0 = max_x;
    } else {
        knots.push((max_x, 1.0));
    }
    knots
}

fn catmull(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    let t2 = t * t;
    let t3 = t2 * t;
    0.5 * (2.0 * p1
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, freq: f64, weight: f64) -> StrengthNode {
        StrengthNode { id, freq, weight }
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
    fn weights_are_clamped() {
        let nodes = [node(1, 1000.0, 4.0)];
        let w = weight_at(&nodes, 1000.0, 10.0, 22050.0);
        assert!((0.0..=1.0).contains(&w));
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
