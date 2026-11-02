
use std::collections::{HashMap, HashSet};

pub const EVIDENCE_COUNT: usize = 19;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum EvidenceKind {
    Kelp = 0,
    CaveVines = 1,
    WeepingVines = 2,
    Bamboo = 3,
    SugarCane = 4,
    Cactus = 5,
    Cocoa = 6,
    SweetBerries = 7,
    NetherWart = 8,
    Crops = 9,
    Stems = 10,
    Amethyst = 11,
    Vines = 12,
    Leaves = 13,
    GrassAndDirt = 14,
    MangrovePropagules = 15,
    Eyeblossoms = 16,
    Snow = 17,
    Ice = 18,
}

#[derive(Clone, Debug)]
pub struct BetterSusConfig {
    pub simulation_distance: i32,
    pub min_flag: i32,
    pub hole_filter: bool,
    pub hole_distance: i32,
}

impl Default for BetterSusConfig {
    fn default() -> Self {
        Self {
            simulation_distance: 4,
            min_flag: 40,
            hole_filter: false,
            hole_distance: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
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

    pub fn config(&self) -> &BetterSusConfig {
        &self.config
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
            hole: observation.has_hole || old.is_some_and(|r| r.hole),
            weight: weight.max(old.map_or(0, |r| r.weight)),
        };

        self.scanned.insert(key);
        self.records.insert(key, record);

        if record.clears {
            self.add_coverage(key, record);
        }
    }

    fn add_coverage(
        &mut self,
        (cx, cz): (i32, i32),
        record: ChunkRecord,
    ) {
        for dx in -record.radius..=record.radius {
            for dz in -record.radius..=record.radius {
                *self.clear_coverage
                    .entry((cx + dx, cz + dz))
                    .or_insert(0) += 1;
            }
        }

        let reach = 2 * record.radius + 2;

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

                for dx in -record.radius..=record.radius {
                    for dz in -record.radius..=record.radius {
                        Self::subtract(
                            &mut self.clear_coverage,
                            (cx + dx, cz + dz),
                            1,
                        );
                    }
                }

                let reach = 2 * record.radius + 2;

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

    fn covered_sides(
        &self,
        cx: i32,
        cz: i32,
        reach: i32,
    ) -> i32 {
        [(1, 0), (-1, 0), (0, 1), (0, -1)]
            .iter()
            .filter(|&&(dx, dz)| {
                self.strip_cleared(cx, cz, dx, dz, reach)
            })
            .count() as i32
    }

    fn hole_nearby(
        &self,
        cx: i32,
        cz: i32,
        distance: i32,
    ) -> bool {
        for dx in -distance..=distance {
            for dz in -distance..=distance {
                if self.records
                    .get(&(cx + dx, cz + dz))
                    .is_some_and(|r| r.hole)
                {
                    return true;
                }
            }
        }

        false
    }

    pub fn flagged_chunks(&self) -> Vec<(i32, i32)> {
        let threshold = self.config.min_flag.max(1);
        let hole_distance = self.config.hole_distance.clamp(0, 8);
        let reach =
            2 * self.config.simulation_distance.clamp(2, 16) + 2;

        let mut result = Vec::new();

        for &(cx, cz) in &self.scanned {
            let key = (cx, cz);

            if self.clear_coverage.contains_key(&key) {
                continue;
            }

            let score = self.support
                .get(&key)
                .copied()
                .unwrap_or(0);

            if score < threshold {
                continue;
            }

            if self.covered_sides(cx, cz, reach) < 3 {
                continue;
            }

            if self.config.hole_filter
                && self.hole_nearby(cx, cz, hole_distance)
            {
                continue;
            }

            result.push(key);
        }

        result.sort_unstable();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_indices_are_correct() {
        assert_eq!(EvidenceKind::Kelp as usize, 0);
        assert_eq!(EvidenceKind::Amethyst as usize, 11);
        assert_eq!(EvidenceKind::Eyeblossoms as usize, 16);
        assert_eq!(EvidenceKind::Ice as usize, 18);
        assert_eq!(EVIDENCE_COUNT, 19);
    }

    #[test]
    fn reset_clears_all_data() {
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
        assert!(engine.records.is_empty());
        assert!(engine.clear_coverage.is_empty());
        assert!(engine.support.is_empty());
    }

    #[test]
    fn empty_observation_adds_no_coverage() {
        let mut engine =
            BetterSusEngine::new(BetterSusConfig::default());

        engine.observe(ChunkObservation {
            x: 0,
            z: 0,
            overdue_count: 0,
            has_hole: false,
        });

        assert!(engine.clear_coverage.is_empty());
        assert!(engine.support.is_empty());
    }
}
