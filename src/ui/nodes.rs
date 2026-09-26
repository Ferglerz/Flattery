use super::*;

impl FlatteryView {
    pub(super) fn sync_scale(&mut self) {
        self.layout.set_view(
            self.params.max_boost_db.value(),
            self.params.max_cut_db.value(),
            self.params.low_cut_hz.value() as f64,
            self.params.high_cut_hz.value() as f64,
            self.graph_zoomed,
        );
    }

    pub(super) fn strength_pct(&self, polarity: Polarity) -> f32 {
        match polarity {
            Polarity::Boost => self.params.strength_boost.value(),
            Polarity::Cut => self.params.strength_cut.value(),
        }
    }

    pub(super) fn bin_hz(&self) -> f64 {
        let fft = self.params.fft_size.value().size();
        let srate = self.shared.sample_rate.load(Ordering::Relaxed) as f64;
        if fft == 0 || srate <= 0.0 {
            0.0
        } else {
            srate / fft as f64
        }
    }

    pub(super) fn snap_hz(&self, freq: f64) -> f64 {
        snap_to_bin_center(freq, self.bin_hz())
    }

    pub(super) fn with_nodes_mut<R>(
        &self,
        polarity: Polarity,
        f: impl FnOnce(&mut Vec<StrengthNode>) -> R,
    ) -> R {
        let lock = match polarity {
            Polarity::Boost => &self.params.boost_nodes,
            Polarity::Cut => &self.params.cut_nodes,
        };
        let (result, snapshot) = match lock.lock() {
            Ok(mut guard) => {
                let result = f(&mut guard);
                (result, Arc::<[StrengthNode]>::from(guard.clone()))
            }
            Err(poisoned) => {
                let mut guard = poisoned.into_inner();
                let result = f(&mut guard);
                (result, Arc::<[StrengthNode]>::from(guard.clone()))
            }
        };
        self.shared
            .publish_nodes(polarity == Polarity::Boost, snapshot);
        result
    }

    pub(super) fn create_node(&mut self, polarity: Polarity, x: f32) -> u64 {
        let freq = self.snap_hz(self.layout.x_to_freq(x));
        let min_freq = self.layout.min_freq;
        let max_freq = self.layout.max_freq;
        self.with_nodes_mut(polarity, |nodes| {
            let id = next_node_id(nodes);
            let weight = weight_at(nodes, freq, min_freq, max_freq);
            nodes.push(StrengthNode::new(id, freq, weight));
            id
        })
    }

    pub(super) fn delete_selected(&mut self) {
        if let Some((polarity, id)) = self.selected.take() {
            self.with_nodes_mut(polarity, |nodes| {
                nodes.retain(|n| n.id != id);
            });
        }
    }
}

/// Match Damian's relative Shift-drag sensitivity and downward-increases-Q direction.
pub(super) fn drag_strength_node(
    node: &mut StrengthNode,
    freq: f64,
    weight: f64,
    dy: f32,
    shift: bool,
) {
    if shift {
        node.q *= 1.0 + f64::from(dy) * 0.025;
    } else {
        node.freq = freq;
        node.weight = weight;
    }
    node.sanitize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_drag_changes_only_q_and_normal_drag_preserves_q() {
        let mut node = StrengthNode::new(1, 1000.0, 1.0);
        drag_strength_node(&mut node, 2000.0, 2.0, 20.0, true);
        assert_eq!((node.freq, node.weight, node.q), (1000.0, 1.0, 9.0));
        drag_strength_node(&mut node, 2000.0, 2.0, -20.0, true);
        assert_eq!(node.q, 4.5);
        drag_strength_node(&mut node, 2000.0, 2.0, 0.0, false);
        assert_eq!((node.freq, node.weight, node.q), (2000.0, 2.0, 4.5));
        drag_strength_node(&mut node, 2000.0, 2.0, -1000.0, true);
        assert_eq!(node.q, crate::strength::MIN_NODE_Q);
        drag_strength_node(&mut node, 2000.0, 2.0, 1000.0, true);
        assert_eq!(node.q, crate::strength::MAX_NODE_Q);
    }
}
