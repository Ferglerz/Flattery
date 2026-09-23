use super::*;

impl GraphLayout {
    pub fn hit_strength_handle(
        &self,
        polarity: Polarity,
        strength_pct: f32,
        x: f32,
        y: f32,
    ) -> bool {
        let (hx, hy) = self.strength_handle_pos(polarity, strength_pct);
        tag_contains(hx, hy, TagPointer::Right, x, y)
    }

    pub fn hit_max_line(&self, x: f32, y: f32, line_y: f32) -> bool {
        x >= self.gx
            && x <= self.gx + self.gw
            && (y - line_y).abs() <= HIT_DIST
            && y >= self.gy
            && y <= self.gy + self.gh
    }

    pub fn hit_max_boost(&self, x: f32, y: f32, max_boost_db: f32) -> bool {
        self.hit_max_line(x, y, self.db_to_y(max_boost_db as f64))
    }

    pub fn hit_max_cut(&self, x: f32, y: f32, max_cut_db: f32) -> bool {
        self.hit_max_line(x, y, self.db_to_y(-(max_cut_db as f64)))
    }

    pub fn hit_op_min(&self, x: f32, y: f32, op_min_db: f32) -> bool {
        let hy = self.mag_to_y(op_min_db as f64);
        x >= self.gx && x <= self.gx + 32.0 && (y - hy).abs() <= HIT_DIST
    }

    pub fn hit_op_max(&self, x: f32, y: f32, op_max_db: f32) -> bool {
        let hy = self.mag_to_y(op_max_db as f64);
        x >= self.gx && x <= self.gx + 32.0 && (y - hy).abs() <= HIT_DIST
    }

    pub fn hit_node(
        &self,
        nodes: &[StrengthNode],
        polarity: Polarity,
        strength_pct: f32,
        _max_boost_db: f32,
        _max_cut_db: f32,
        x: f32,
        y: f32,
    ) -> Option<u64> {
        let mut best = None;
        let mut best_d = crate::strength::NODE_HIT_R;
        for node in nodes {
            let nx = self.freq_to_x(node.freq);
            let ny = self.strength_y(polarity, strength_pct, node.weight);
            let d = ((x - nx).powi(2) + (y - ny).powi(2)).sqrt();
            let hit_r = crate::strength::NODE_HIT_R;
            if d <= hit_r && d <= best_d {
                best_d = d;
                best = Some(node.id);
            }
        }
        best
    }

    pub fn hit_cut_line(&self, x: f32, y: f32, cut_x: f32) -> bool {
        let line_hit = (x - cut_x).abs() <= HIT_DIST && y >= self.gy && y <= self.gy + self.gh;
        line_hit || tag_contains(cut_x, self.gy, TagPointer::Down, x, y)
    }

}
