// Maza Client: StorageEsp, SusChunkFinder (+BetterSus, ANBS+ glow), AntiVanish, Freecam, FreeLook.
// This file is both the Cargo library (tests) and the module added to the game.
use std::collections::{HashMap, HashSet};

pub const ID: &str = "maza";
pub const NAME: &str = "Maza Client";
pub const VERSION: &str = "0.1.0";

// ---------- shared state ----------
#[derive(Default)]
pub struct State {
    pub storage: bool,
    pub sus: bool,
    pub vanish: bool,
    pub freecam: bool,
    pub freelook: bool,
    pub tick: u64,
    pub boxes: Vec<(f64, f64, f64, u32, usize)>,
    pub shown: Vec<(f64, f64, f64, u32, usize)>,
    pub pending_scores: HashMap<(i32, i32), i32>,
    pub scan_queue: Vec<(i32, i32)>,
    pub st_flip: [bool; 8],
    pub st_tracers: bool,
    pub det: HashMap<String, HashMap<(i32, i32, i32), u8>>,
    pub st_scanned: HashSet<(String, i32, i32)>,
    pub st_check: usize,
    pub scanner: ChunkScanner,
    pub heat: Heat,
    pub bsc_on: bool,
    pub bsc: Option<bsc::BetterSusEngine>,
    pub bsc_flagged: Vec<(i32, i32)>,
    pub sus_flagged: Vec<((i32, i32), i32)>,
    pub sus_anbs: bool,
    pub sus_sens: i32,
    pub sus_radius: i32,
    pub bsc_sens: i32,
    pub bsc_sim: i32,
    pub chunk_scores: HashMap<(i32, i32), i32>,
    pub glow: HashMap<(i32, i32), i32>,
    pub known_tab: HashMap<u128, String>,
    pub gone_at: HashMap<u128, u64>,
    pub alerts: Vec<(u64, String)>,
    pub cam: [f64; 3],
    pub cam_yaw: f32,
    pub cam_pitch: f32,
    pub keys: [bool; 6], // w a s d space shift
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
}
thread_local! { pub static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State::default()); }
pub fn with<R>(f: impl FnOnce(&mut State) -> R) -> R { STATE.with(|s| f(&mut s.borrow_mut())) }

pub fn type_enabled(s: &State, i: usize) -> bool { STORAGE_DEFAULT_ON[i] != s.st_flip[i] }

pub fn toggle(name: &str) -> Option<(&'static str, bool)> {
    with(|s| {
        let (label, flag): (&'static str, &mut bool) = match name {
            "storage" | "storageesp" => ("StorageEsp", &mut s.storage),
            "sus" | "suschunk" | "suschunkfinder" => ("SusChunkFinder", &mut s.sus),
            "bsc" | "bettersus" | "bettersuschunk" => ("BetterSusChunk", &mut s.bsc_on),
            "vanish" | "antivanish" => ("AntiVanish", &mut s.vanish),
            "freecam" => ("Freecam", &mut s.freecam),
            "freelook" | "freelock" => ("FreeLook", &mut s.freelook),
            _ => return None,
        };
        *flag = !*flag;
        let v = *flag;
        Some((label, v))
    })
}

// ---------- storage esp (Anubis types; remembers detections per dimension) ----------
pub const STORAGE_NAMES: [&str; 8] = ["Chest", "Ender Chest", "Shulker Box", "Spawner", "Furnace", "Barrel", "Enchanting Table", "Hopper"];
pub const STORAGE_KEYS: [&str; 8] = ["chest", "ender", "shulker", "spawner", "furnace", "barrel", "enchanting", "hopper"];
pub const STORAGE_COLORS: [u32; 8] = [0xFF9C5B00, 0xFF7500FF, 0xFF86009E, 0xFF8A7EA6, 0xFF646464, 0xFF644B32, 0xFFC80000, 0xFF505050];
pub const STORAGE_DEFAULT_ON: [bool; 8] = [true, true, true, true, false, true, true, false];

pub fn storage_kind(name: &str) -> Option<usize> {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    Some(match n {
        "chest" | "trapped_chest" => 0,
        "ender_chest" => 1,
        "spawner" | "trial_spawner" => 3,
        "furnace" | "blast_furnace" | "smoker" => 4,
        "barrel" => 5,
        "enchanting_table" => 6,
        "hopper" => 7,
        _ if n.ends_with("shulker_box") || n == "shulker_box" => 2,
        _ => return None,
    })
}
/// Detections are kept per dimension and only dropped when the block is seen gone.
pub fn remember(map: &mut HashMap<(i32, i32, i32), u8>, pos: (i32, i32, i32), kind: usize) { map.insert(pos, kind as u8); }
pub fn forget_if_changed(map: &mut HashMap<(i32, i32, i32), u8>, pos: (i32, i32, i32), now: Option<usize>) {
    if now != map.get(&pos).map(|k| *k as usize) { match now { Some(k) => { map.insert(pos, k as u8); } None => { map.remove(&pos); } } }
}

// ---------- sus chunk finder ----------
/// Evidence weights: blocks that only reach a grown state when their chunk was
/// loaded and ticked while someone was near (BetterSus idea, simplified).
pub fn growth_weight(name: &str) -> i32 {
    let n = name.strip_prefix("minecraft:").unwrap_or(name);
    match n {
        "kelp_plant" | "weeping_vines_plant" | "cave_vines_plant" | "twisting_vines_plant" => 3,
        "bamboo" | "sugar_cane" | "cactus" => 2,
        "budding_amethyst" => 6,
        "large_amethyst_bud" | "medium_amethyst_bud" | "small_amethyst_bud" | "amethyst_cluster" => 4,
        "nether_wart" | "sweet_berry_bush" | "cocoa" | "pumpkin" | "melon" => 1,
        _ => 0,
    }
}
pub fn sus_flagged(scores: &HashMap<(i32, i32), i32>, min_flag: i32) -> Vec<((i32, i32), i32)> {
    let mut v: Vec<_> = scores.iter().filter(|(_, s)| **s >= min_flag).map(|(k, s)| (*k, *s)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}
/// ANBS+ glow: a chunk with amethyst whose cluster block light is higher than
/// the neighbouring air suggests a hidden geode (light 5 for a cluster, 4 for large bud...).
pub fn amethyst_glow(name: &str) -> i32 {
    match name.strip_prefix("minecraft:").unwrap_or(name) {
        "amethyst_cluster" => 5,
        "large_amethyst_bud" => 4,
        "medium_amethyst_bud" => 2,
        "small_amethyst_bud" => 1,
        _ => 0,
    }
}

// ---------- anti vanish ----------
/// Compare tab-list snapshots. A name that disappears is reported; a name that
/// reappears after a silent disappearance within `window` ticks is a likely vanish toggle.
pub fn vanish_update(known: &mut HashMap<u128, String>, now: &[(u128, String)], tick: u64, gone_at: &mut HashMap<u128, u64>, window: u64) -> Vec<String> {
    let mut out = Vec::new();
    let present: HashSet<u128> = now.iter().map(|p| p.0).collect();
    for (u, name) in now {
        if known.insert(*u, name.clone()).is_none() {
            if let Some(t) = gone_at.remove(u) {
                if tick.saturating_sub(t) <= window {
                    out.push(format!("{name}: tab'a geri döndü (vanish kapatmış olabilir)"));
                }
            }
        }
    }
    let gone: Vec<u128> = known.keys().filter(|u| !present.contains(u)).copied().collect();
    for u in gone {
        if let Some(name) = known.remove(&u) {
            gone_at.insert(u, tick);
            out.push(format!("{name}: tab'dan çıktı (çıkış veya vanish)"));
        }
    }
    out
}

// ---------- camera ----------
pub fn forward(yaw: f32, pitch: f32) -> [f64; 3] {
    let (y, p) = (f64::from(yaw).to_radians(), f64::from(pitch).to_radians());
    [-y.sin() * p.cos(), -p.sin(), y.cos() * p.cos()]
}
/// keys: w a s d space shift
pub fn freecam_step(pos: [f64; 3], yaw: f32, pitch: f32, keys: [bool; 6], speed: f64, dt: f64) -> [f64; 3] {
    let f = forward(yaw, pitch);
    let y = f64::from(yaw).to_radians();
    let r = [-y.cos(), 0.0, -y.sin()];
    let mut m = [0.0f64; 3];
    let mut add = |v: [f64; 3], k: f64| { for i in 0..3 { m[i] += v[i] * k; } };
    if keys[0] { add(f, 1.0); }
    if keys[2] { add(f, -1.0); }
    if keys[3] { add(r, 1.0); }
    if keys[1] { add(r, -1.0); }
    if keys[4] { add([0.0, 1.0, 0.0], 1.0); }
    if keys[5] { add([0.0, 1.0, 0.0], -1.0); }
    let len = (m[0] * m[0] + m[1] * m[1] + m[2] * m[2]).sqrt();
    if len < 1e-6 { return pos; }
    let k = speed * dt / len;
    [pos[0] + m[0] * k, pos[1] + m[1] * k, pos[2] + m[2] * k]
}
pub fn orbit_position(eye: [f64; 3], yaw: f32, pitch: f32, dist: f64) -> [f64; 3] {
    let f = forward(yaw, pitch);
    [eye[0] - f[0] * dist, eye[1] - f[1] * dist, eye[2] - f[2] * dist]
}

// ---------- 3D -> screen (for through-wall ESP on the HUD) ----------
pub fn project(cam: [f64; 3], yaw: f32, pitch: f32, fov_deg: f32, w: f32, h: f32, p: [f64; 3]) -> Option<(f32, f32)> {
    let d = [p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]];
    let f = forward(yaw, pitch);
    let yr = f64::from(yaw).to_radians();
    let right = [-yr.cos(), 0.0, -yr.sin()];
    // up = right x forward
    let up = [right[1] * f[2] - right[2] * f[1], right[2] * f[0] - right[0] * f[2], right[0] * f[1] - right[1] * f[0]];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let z = dot(d, f);
    if z < 0.05 { return None; }
    let t = (f64::from(fov_deg).to_radians() / 2.0).tan();
    let aspect = f64::from(w / h);
    let sx = dot(d, right) / (z * t * aspect);
    let sy = dot(d, up) / (z * t);
    // right vector points to screen-left in vanilla yaw convention: flip
    Some(((w / 2.0 + (-sx as f32) * w / 2.0), (h / 2.0 - sy as f32 * h / 2.0)))
}


// ---------- block info + world view (filled by the Horizon glue) ----------
use std::rc::Rc;

#[derive(Clone, Default, Debug)]
pub struct Info {
    pub name: String,
    pub props: Vec<(String, String)>,
    pub air: bool,
    /// BetterSus kind (-1 none), SusFinder kind (-1 none), storage kind (-1 none)
    pub bs: i8,
    pub sf: i8,
    pub storage: i8,
}
impl Info {
    pub fn new(name: &str, props: Vec<(String, String)>) -> Info {
        let n = name.strip_prefix("minecraft:").unwrap_or(name).to_string();
        let mut i = Info { name: n, props, air: false, bs: -1, sf: -1, storage: -1 };
        i.air = matches!(i.name.as_str(), "air" | "cave_air" | "void_air");
        i.bs = bs_classify(&i);
        i.sf = sf_kind(&i);
        i.storage = storage_kind(&i.name).map_or(-1, |k| k as i8);
        i
    }
    pub fn p(&self, k: &str) -> Option<&str> { self.props.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str()) }
    pub fn int(&self, k: &str) -> Option<i32> { self.p(k).and_then(|v| v.parse().ok()) }
    pub fn flag(&self, k: &str) -> bool { self.p(k) == Some("true") }
    pub fn is(&self, n: &str) -> bool { self.name == n }
    pub fn is_bud(&self) -> bool { matches!(self.name.as_str(), "small_amethyst_bud" | "medium_amethyst_bud" | "large_amethyst_bud") }
    pub fn water(&self) -> bool { self.name == "water" }
    pub fn full_water(&self) -> bool { self.name == "water" && self.int("level") == Some(0) }
}
pub trait View {
    fn info(&self, x: i32, y: i32, z: i32) -> Rc<Info>;
    /// max(block light, sky light)
    fn light(&self, x: i32, y: i32, z: i32) -> u8;
    /// block light only
    fn block_light(&self, x: i32, y: i32, z: i32) -> u8;
}

pub struct FnView<'a> {
    pub info: &'a dyn Fn(i32, i32, i32) -> Rc<Info>,
    pub light: &'a dyn Fn(i32, i32, i32) -> u8,
    pub block_light: &'a dyn Fn(i32, i32, i32) -> u8,
}
impl View for FnView<'_> {
    fn info(&self, x: i32, y: i32, z: i32) -> Rc<Info> { (self.info)(x, y, z) }
    fn light(&self, x: i32, y: i32, z: i32) -> u8 { (self.light)(x, y, z) }
    fn block_light(&self, x: i32, y: i32, z: i32) -> u8 { (self.block_light)(x, y, z) }
}

/// Defaults (Anubis): sus radius 5 / sensitivity 6, BetterSus sim distance 4 / sensitivity 40.
pub fn ensure_defaults(s: &mut State) {
    if s.sus_radius == 0 {
        s.sus_radius = 5;
        s.sus_sens = 6;
        s.bsc_sens = 40;
        s.bsc_sim = 4;
        s.heat.radius = 5;
        s.scanner.rescan_after = 1200;
        s.bsc = Some(bsc::BetterSusEngine::new(bsc::BetterSusConfig { simulation_distance: 4, min_flag: 40, hole_filter: false, hole_distance: 2 }));
    }
}

fn facing_delta(f: &str) -> (i32, i32, i32) {
    match f { "up" => (0, 1, 0), "down" => (0, -1, 0), "north" => (0, 0, -1), "south" => (0, 0, 1), "west" => (-1, 0, 0), "east" => (1, 0, 0), _ => (0, 0, 0) }
}

// ---------- BetterSus classification (Anubis kinds 0..=15; 14,16,17,18 not ported) ----------
pub fn bs_classify(i: &Info) -> i8 {
    match i.name.as_str() {
        "kelp" => 0,
        "cave_vines" => 1,
        "weeping_vines" => 2,
        "bamboo" => 3,
        "sugar_cane" => 4,
        "cactus" => 5,
        "cocoa" => 6,
        "sweet_berry_bush" => 7,
        "nether_wart" => 8,
        "pumpkin_stem" | "melon_stem" => 10,
        "vine" => 12,
        "budding_amethyst" | "amethyst_cluster" => 11,
        "mangrove_propagule" => if i.flag("hanging") { 15 } else { -1 },
        "wheat" | "carrots" | "potatoes" | "beetroots" | "torchflower_crop" => 9,
        n if n.ends_with("_leaves") => 13,
        _ if i.is_bud() => 11,
        _ => -1,
    }
}
fn crop_max(name: &str) -> i32 { match name { "beetroots" => 3, "torchflower_crop" => 1, _ => 7 } }

fn column<V: View + ?Sized>(v: &V, x: i32, y: i32, z: i32, target: &str, limit: i32) -> i32 {
    let mut l = 1;
    while l < limit && v.info(x, y - l, z).is(target) { l += 1; }
    l
}
/// Anubis `growthOwed`: the block should have grown/decayed by now if its chunk had been simulated.
pub fn bs_owed<V: View + ?Sized>(v: &V, x: i32, y: i32, z: i32, i: &Info) -> bool {
    let above = || v.info(x, y + 1, z);
    let below = || v.info(x, y - 1, z);
    match i.name.as_str() {
        "kelp" => i.int("age").unwrap_or(25) < 25 && above().water(),
        "cave_vines" | "weeping_vines" => i.int("age").unwrap_or(25) < 25 && below().air,
        "bamboo" => {
            let mut b = 0;
            while b < 16 && v.info(x, y - b - 1, z).is("bamboo") { b += 1; }
            i.int("age") == Some(0) && above().air && b + 1 < 16 && v.light(x, y + 1, z) >= 9
        }
        "sugar_cane" => above().air && column(v, x, y, z, "sugar_cane", 3) < 3,
        "cactus" => above().air && (i.int("age").unwrap_or(15) < 15 || column(v, x, y, z, "cactus", 3) < 3),
        "cocoa" => i.int("age").unwrap_or(2) < 2,
        "nether_wart" => i.int("age").unwrap_or(3) < 3,
        "sweet_berry_bush" => i.int("age").unwrap_or(3) < 3 && v.light(x, y + 1, z) >= 9,
        "pumpkin_stem" | "melon_stem" => i.int("age").unwrap_or(7) < 7 && v.light(x, y, z) >= 9,
        "wheat" | "carrots" | "potatoes" | "beetroots" | "torchflower_crop" => i.int("age").unwrap_or(99) < crop_max(&i.name) && v.light(x, y, z) >= 9,
        "vine" => below().air && ["up", "north", "south", "east", "west"].iter().any(|f| i.flag(f)),
        "budding_amethyst" => {
            for f in ["up", "down", "north", "south", "east", "west"] {
                let (dx, dy, dz) = facing_delta(f);
                let n = v.info(x + dx, y + dy, z + dz);
                if n.air || n.full_water() { return true; }
            }
            false
        }
        _ if i.is_bud() => {
            if let Some(f) = i.p("facing") {
                let (dx, dy, dz) = facing_delta(f);
                v.info(x - dx, y - dy, z - dz).is("budding_amethyst")
            } else { false }
        }
        n if n.ends_with("_leaves") => i.int("distance") == Some(7) && !i.flag("persistent"),
        _ => false,
    }
}

// ---------- SusChunkFinder (Classic): kinds 0..=6, sign +1 grown / -1 could still grow ----------
pub fn sf_kind(i: &Info) -> i8 {
    match i.name.as_str() {
        "amethyst_cluster" => 0,
        _ if i.is_bud() => 0,
        "kelp" => 1,
        "cave_vines" => 2,
        "vine" => 3,
        "bamboo" => 4,
        "cocoa" => 5,
        "bee_nest" | "beehive" => 6,
        _ => -1,
    }
}
pub fn sf_sign<V: View + ?Sized>(v: &V, x: i32, y: i32, z: i32, i: &Info, kind: i8) -> i32 {
    match kind {
        0 => {
            if i.is("amethyst_cluster") { return 1; }
            let Some(f) = i.p("facing") else { return 0 };
            let (dx, dy, dz) = facing_delta(f);
            if v.info(x - dx, y - dy, z - dz).is("budding_amethyst") { -1 } else { 1 }
        }
        1 => {
            let age = i.int("age").unwrap_or(0);
            if age != 25 && v.info(x, y + 1, z).water() { -1 } else if age == 25 { 1 } else { 0 }
        }
        2 => {
            let age = i.int("age").unwrap_or(0);
            if age != 25 && v.info(x, y - 1, z).air { -1 } else if age == 25 { 1 } else { 0 }
        }
        3 => {
            let b = v.info(x, y - 1, z);
            if b.is("vine") { 0 } else if !b.air { 1 } else if ["north", "south", "east", "west"].iter().any(|f| i.flag(f)) || i.flag("up") { -1 } else { 1 }
        }
        4 => {
            let mut h = 1;
            while h < 16 && v.info(x, y - h, z).is("bamboo") { h += 1; }
            if v.info(x, y + 1, z).air && h >= 16 { 1 } else if i.int("age") == Some(0) && v.info(x, y + 1, z).air && h < 16 { -1 } else { 0 }
        }
        5 => if i.int("age") == Some(2) { 1 } else { -1 },
        6 => if i.int("honey_level") == Some(5) { 1 } else { -1 },
        _ => 0,
    }
}

/// ANBS+: air/amethyst-cluster cells with block light 0 whose neighbours reach light 4 exactly and
/// never above, with a lit open neighbour (Anubis GeodeGlowScan). Count / 12 = heat.
pub fn glow_scan<V: View + ?Sized>(v: &V, cx: i32, cz: i32) -> i32 {
    let open = |i: &Info| i.air || i.is("amethyst_cluster");
    let mut count = 0;
    for y in -58..=30 {
        for z in cz * 16..cz * 16 + 16 {
            for x in cx * 16..cx * 16 + 16 {
                if v.block_light(x, y, z) != 0 || !open(&v.info(x, y, z)) { continue; }
                let mut ok = true;
                let mut brightest = 0;
                let mut lit_open = false;
                for (dx, dy, dz) in [(1, 0, 0), (-1, 0, 0), (0, 1, 0), (0, -1, 0), (0, 0, 1), (0, 0, -1)] {
                    let l = v.block_light(x + dx, y + dy, z + dz);
                    if l > 4 { ok = false; break; }
                    brightest = brightest.max(l);
                    if l == 4 && !lit_open && open(&v.info(x + dx, y + dy, z + dz)) { lit_open = true; }
                }
                if ok && brightest == 4 && lit_open { count += 1; }
            }
        }
    }
    count
}

#[derive(Default, Clone, Debug)]
pub struct ScanOut {
    pub bs_count: i32,
    pub grown: [i32; 7],
    pub ungrown: u32,
    pub storage: Vec<((i32, i32, i32), usize)>,
}
/// One pass over a chunk for all modules.
pub fn scan_chunk<V: View + ?Sized>(v: &V, cx: i32, cz: i32, bottom: i32, top: i32, want_bs: bool, want_sf: bool, want_storage: bool) -> ScanOut {
    let mut out = ScanOut::default();
    for x in cx * 16..cx * 16 + 16 {
        for z in cz * 16..cz * 16 + 16 {
            for y in bottom..=top {
                let i = v.info(x, y, z);
                if i.air { continue; }
                if want_storage && i.storage >= 0 { out.storage.push(((x, y, z), i.storage as usize)); }
                if want_bs && i.bs >= 0 {
                    if i.bs == 15 { if i.int("age").unwrap_or(4) < 4 { out.bs_count += 1; } }
                    else if bs_owed(v, x, y, z, &i) { out.bs_count += 1; }
                }
                if want_sf && i.sf >= 0 {
                    let s = sf_sign(v, x, y, z, &i, i.sf);
                    if s == 1 { out.grown[i.sf as usize] += 1; } else if s == -1 { out.ungrown |= 1 << i.sf; }
                }
            }
        }
    }
    out
}

// ---------- SusChunkFinder heat maps ----------
#[derive(Default)]
pub struct Heat {
    pub heat: [HashMap<(i32, i32), i32>; 7],
    pub vetoes: [HashMap<(i32, i32), i32>; 7],
    pub chunks: HashMap<(i32, i32), ([i32; 7], u32)>,
    pub scanned: HashSet<(i32, i32)>,
    pub radius: i32,
}
fn area(cx: i32, cz: i32, r: i32, mut f: impl FnMut((i32, i32))) { for dx in -r..=r { for dz in -r..=r { f((cx + dx, cz + dz)); } } }
fn sub(m: &mut HashMap<(i32, i32), i32>, k: (i32, i32), a: i32) { if let Some(v) = m.get_mut(&k) { *v -= a; if *v <= 0 { m.remove(&k); } } }
impl Heat {
    pub fn withdraw(&mut self, cx: i32, cz: i32) {
        if let Some((g, u)) = self.chunks.remove(&(cx, cz)) {
            let r = self.radius;
            for k in 0..7 {
                if g[k] > 0 { area(cx, cz, r, |a| sub(&mut self.heat[k], a, g[k])); }
                if u & (1 << k) != 0 { area(cx, cz, r, |a| sub(&mut self.vetoes[k], a, 1)); }
            }
        }
    }
    pub fn update(&mut self, cx: i32, cz: i32, grown: [i32; 7], ungrown: u32) {
        self.withdraw(cx, cz);
        let r = self.radius;
        for k in
