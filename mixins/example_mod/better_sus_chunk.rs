
use std::collections::{HashMap, HashSet};

pub fn block_break_message(pos: [i32; 3]) -> String {
    format!(
        "[example_mod] Breaking block at {}, {}, {}",
        pos[0], pos[1], pos[2]
    )
}

pub fn before_break(pos: [i32; 3]) {
    eprintln!("{}", block_break_message(pos));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum EvidenceKind {
    Kelp = 0,
    CaveVines,
    WeepingVines,
    Bamboo,
    SugarCane,
    Cactus,
    Cocoa,
    SweetBerries,
    NetherWart,
    Crops,
    Stems,
    Amethyst,
    Vines,
    Leaves,
    GrassAndDirt,
    MangrovePropagules,
    Eyeblossoms,
    Snow,
    Ice,
}

pub const EVIDENCE_COUNT: usize = 19;

#[derive(Clone)]
pub struct BetterSusConfig {
    pub simulation_distance: i32,
    pub min_flag: i32,
    pub hole_filter: bool,
    pub hole_distance: i32,
    pub enabled_evidence: [bool; EVIDENCE_COUNT],
}

impl Default for BetterSusConfig {
    fn default() -> Self {
        Self {
            simulation_distance: 4,
            min_flag: 40,
            hole_filter: false,
            hole_distance: 2,
            enabled_evidence: [true; EVIDENCE_COUNT],
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkObservation {
    pub x: i32,
    pub z: i32,
    pub overdue_count: i32,
    pub has_hole: bool,
}

#[derive(Clone, Copy, Debug)]
struct ChunkRecord {
    clears: bool,
    radius: i32,
    hole: bool,
    weight: i32,
}

pub struct BetterSusEngine {
    config: BetterSusConfig,
    records: HashMap<(i32, i32), ChunkRecord>,
    scanned: HashSet<(i32, i32)>,
    clear_coverage: HashMap<(i32, i32), i32>,
    support: HashMap<(i32, i32), i32>,
}

impl BetterSusEngine {
    pub fn new(config: BetterSusConfig) -> Self {
        Self {
            config,
            records: HashMap::new(),
            scanned: HashSet::new(),
            clear_coverage: HashMap::new(),
            support: HashMap::new(),
        }
    }

    pub fn config_mut(&mut self) -> &mut BetterSusConfig {
        &mut self.config
    }

    pub fn reset(&mut self) {
        self.records.clear();
        self.scanned.clear();
        self.clear_coverage.clear();
        self.support.clear();
    }

    pub fn observe(&mut self, observation: ChunkObservation) {
        let key = (observation.x, observation.z);
        let old = self.remove_record(key);

        let radius = self.config.simulation_distance.clamp(2, 16);
        let weight = observation.overdue_count.max(0);

        let record = ChunkRecord {
            clears: weight > 0 || old.is_some_and(|r| r.clears),
            radius,
            hole: observation.has_hole
                || old.is_some_and(|r| r.hole),
            weight: weight.max(old.map_or(0, |r| r.weight)),
        };

        self.scanned.insert(key);
        self.records.insert(key, record);

        if record.clears {
            self.add_coverage(key, record);
        }
    }

    fn add_coverage(&mut self, key: (i32, i32), record: ChunkRecord) {
        let (cx, cz) = key;
        let radius = record.radius;
        let reach = 2 * radius + 2;

        for dx in -radius..=radius {
            for dz in -radius..=radius {
                *self.clear_coverage
                    .entry((cx + dx, cz + dz))
                    .or_insert(0) += 1;
            }
        }

        for dx in -reach..=reach {
            for dz in -reach..=reach {
                *self.support
                    .entry((cx + dx, cz + dz))
                    .or_insert(0) += record.weight;
            }
        }
    }

    fn remove_record(
        &mut self,
        key: (i32, i32),
    ) -> Option<ChunkRecord> {
        let old = self.records.remove(&key);
        self.scanned.remove(&key);

        if let Some(record) = old {
            if record.clears {
                let (cx, cz) = key;
                let radius = record.radius;
                let reach = 2 * radius + 2;

                for dx in -radius..=radius {
                    for dz in -radius..=radius {
                        Self::subtract(
                            &mut self.clear_coverage,
                            (cx + dx, cz + dz),
                            1,
                        );
                    }
                }

                for dx in -reach..=reach {
                    for dz in -reach..=reach {
                        Self::subtract(
                            &mut self.support,
                            (cx + dx, cz + dz),
                            record.weight,
                        );
                    }
                }
            }
        }

        old
    }

    fn subtract(
        map: &mut HashMap<(i32, i32), i32>,
        key: (i32, i32),
        amount: i32,
    ) {
        if let Some(value) = map.get_mut(&key) {
            *value -= amount;
            if *value <= 0 {
                map.remove(&key);
            }
        }
    }

    fn strip_cleared(
        &self,
        cx: i32,
        cz: i32,
        dx: i32,
        dz: i32,
        reach: i32,
    ) -> bool {
        for depth in 1..=reach {
            for lateral in -reach..=reach {
                let key = (
                    cx + dx * depth + dz * lateral,
                    cz + dz * depth + dx * lateral,
                );

                if self.clear_coverage.contains_key(&key) {
                    return true;
                }
            }
        }

        false
    }

    fn covered_sides(&self, cx: i32, cz: i32, reach: i32) -> i32 {
        let sides = [
            (1, 0),
            (-1, 0),
            (0, 1),
            (0, -1),
        ];

        sides
            .iter()
            .filter(|&&(dx, dz)| {
                self.strip_cleared(cx, cz, dx, dz, reach)
            })
            .count() as i32
    }

    pub fn flagged_chunks(&self) -> Vec<(i32, i32)> {
        let threshold = self.config.min_flag.max(1);
        let hole_distance = self.config.hole_distance.clamp(0, 8);
        let reach = 2 * self.config.simulation_distance.clamp(2, 16) + 2;

        let mut result = Vec::new();

        for &(cx, cz) in &self.scanned {
            if self.clear_coverage.contains_key(&(cx, cz)) {
                continue;
            }

            if self.support.get(&(cx, cz)).copied().unwrap_or(0)
                < threshold
            {
                continue;
            }

            if self.covered_sides(cx, cz, reach) < 3 {
                continue;
            }

            if self.config.hole_filter {
                let mut nearby_hole = false;

                for dx in -hole_distance..=hole_distance {
                    for dz in -hole_distance..=hole_distance {
                        if self.records
                            .get(&(cx + dx, cz + dz))
                            .is_some_and(|record| record.hole)
                        {
                            nearby_hole = true;
                            break;
                        }
                    }

                    if nearby_hole {
                        break;
                    }
                }

                if nearby_hole {
                    continue;
                }
            }

            result.push((cx, cz));
        }

        result.sort_unstable();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn break_message_keeps_coordinates() {
        assert_eq!(
            block_break_message([-3, 64, 12]),
            "[example_mod] Breaking block at -3, 64, 12"
        );
    }

    #[test]
    fn reset_removes_all_chunk_data() {
        let mut engine =
            BetterSusEngine::new(BetterSusConfig::default());

        engine.observe(ChunkObservation {
            x: 10,
            z: 10,
            overdue_count: 50,
            has_hole: false,
        });

        engine.reset();

        assert!(engine.flagged_chunks().is_empty());
    }
          }
