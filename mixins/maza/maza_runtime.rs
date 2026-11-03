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
        for k in 0..7 {
            if grown[k] > 0 { area(cx, cz, r, |a| *self.heat[k].entry(a).or_insert(0) += grown[k]); }
            if ungrown & (1 << k) != 0 { area(cx, cz, r, |a| *self.vetoes[k].entry(a).or_insert(0) += 1); }
        }
        self.chunks.insert((cx, cz), (grown, ungrown));
        self.scanned.insert((cx, cz));
    }
    fn neighbours(&self, cx: i32, cz: i32) -> i32 {
        let mut n = 0;
        for dx in -1..=1 { for dz in -1..=1 { if (dx != 0 || dz != 0) && self.scanned.contains(&(cx + dx, cz + dz)) { n += 1; } } }
        n
    }
    /// Anubis `computeVisible`: heat without vetoes, enough scanned neighbours, over the
    /// sensitivity and a local peak.
    pub fn visible(&self, sensitivity: i32, loaded: impl Fn(i32, i32) -> bool) -> Vec<((i32, i32), i32)> {
        let mut total: HashMap<(i32, i32), i32> = HashMap::new();
        for k in 0..7 {
            for (c, h) in &self.heat[k] { if !self.vetoes[k].contains_key(c) { *total.entry(*c).or_insert(0) += *h; } }
        }
        let eligible: HashMap<_, _> = total.into_iter().filter(|(c, _)| loaded(c.0, c.1) && self.neighbours(c.0, c.1) >= 3).collect();
        let reach = self.radius;
        let mut out = Vec::new();
        for (c, v) in &eligible {
            if *v < sensitivity { continue; }
            let mut peak = true;
            'p: for dx in -reach..=reach { for dz in -reach..=reach {
                if (dx != 0 || dz != 0) && eligible.get(&(c.0 + dx, c.1 + dz)).is_some_and(|o| o > v) { peak = false; break 'p; }
            } }
            if peak { out.push((*c, *v)); }
        }
        out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        out
    }
}

// ---------- ChunkScanner: one shared queue, nearest first, periodic rescans ----------
#[derive(Default)]
pub struct ChunkScanner {
    pub last: HashMap<(i32, i32), u64>,
    pub rescan_after: u64,
}
impl ChunkScanner {
    pub fn next(&mut self, pcx: i32, pcz: i32, radius: i32, tick: u64, loaded: impl Fn(i32, i32) -> bool) -> Option<(i32, i32)> {
        let mut best: Option<((i32, i32), i32, bool)> = None;
        for dx in -radius..=radius {
            for dz in -radius..=radius {
                let c = (pcx + dx, pcz + dz);
                let fresh = !self.last.contains_key(&c);
                let due = fresh || tick.saturating_sub(self.last[&c]) >= self.rescan_after.max(1);
                if !due || !loaded(c.0, c.1) { continue; }
                let d = dx * dx + dz * dz;
                if best.is_none_or(|b| (fresh && !b.2) || (fresh == b.2 && d < b.1)) { best = Some((c, d, fresh)); }
            }
        }
        let (c, _, _) = best?;
        self.last.insert(c, tick);
        Some(c)
    }
    pub fn forget_far(&mut self, pcx: i32, pcz: i32, keep: i32) -> Vec<(i32, i32)> {
        let far: Vec<_> = self.last.keys().filter(|c| (c.0 - pcx).abs() > keep || (c.1 - pcz).abs() > keep).copied().collect();
        for c in &far { self.last.remove(c); }
        far
    }
}

pub mod bsc {

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

}

#[cfg(test)]
mod tests {
    use super::*;

    struct Grid(HashMap<(i32,i32,i32), Rc<Info>>, Rc<Info>);
    impl View for Grid {
        fn info(&self, x: i32, y: i32, z: i32) -> Rc<Info> { self.0.get(&(x, y, z)).cloned().unwrap_or_else(|| self.1.clone()) }
        fn light(&self, _: i32, _: i32, _: i32) -> u8 { 15 }
        fn block_light(&self, _: i32, _: i32, _: i32) -> u8 { 0 }
    }
    fn grid() -> Grid { Grid(HashMap::new(), Rc::new(Info::new("minecraft:air", vec![]))) }
    #[test]
    fn young_cane_is_owed_old_is_not() {
        let mut g = grid();
        g.0.insert((0, 0, 0), Rc::new(Info::new("sugar_cane", vec![])));
        let i = g.info(0, 0, 0);
        assert!(bs_owed(&g, 0, 0, 0, &i)); // 1 tall: should have grown
        for y in 1..3 { g.0.insert((0, y, 0), Rc::new(Info::new("sugar_cane", vec![]))); }
        let top = g.info(0, 2, 0);
        assert!(!bs_owed(&g, 0, 2, 0, &top)); // 3 tall: done
    }
    #[test]
    fn scan_counts_and_stores() {
        let mut g = grid();
        g.0.insert((1, 5, 1), Rc::new(Info::new("nether_wart", vec![("age".into(), "0".into())])));
        g.0.insert((2, 5, 2), Rc::new(Info::new("minecraft:chest", vec![])));
        g.0.insert((3, 5, 3), Rc::new(Info::new("cocoa", vec![("age".into(), "2".into())])));
        let o = scan_chunk(&g, 0, 0, 0, 10, true, true, true);
        assert_eq!(o.bs_count, 1);
        assert_eq!(o.storage.len(), 1);
        assert_eq!(o.grown[5], 1);
    }
    #[test]
    fn heat_peak_and_veto() {
        let mut h = Heat { radius: 2, ..Default::default() };
        for dx in -1..=1 { for dz in -1..=1 { h.scanned.insert((10 + dx, 10 + dz)); } }
        for dx in -3..=3 { for dz in -3..=3 { h.scanned.insert((dx, dz)); } }
        h.update(10, 10, [9, 0, 0, 0, 0, 0, 0], 0);
        let v = h.visible(6, |_, _| true);
        assert_eq!(v.first().map(|x| x.1), Some(9));
        assert!(v.len() >= 1);
        h.update(10, 10, [9, 0, 0, 0, 0, 0, 0], 1); // veto on kind 0
        assert!(h.visible(6, |_, _| true).is_empty());
    }
    #[test]
    fn scanner_nearest_first_and_rescan() {
        let mut s = ChunkScanner { rescan_after: 100, ..Default::default() };
        assert_eq!(s.next(0, 0, 3, 0, |_, _| true), Some((0, 0)));
        let second = s.next(0, 0, 3, 1, |_, _| true).unwrap();
        assert!(second.0.abs() <= 1 && second.1.abs() <= 1);
    }

    #[test]
    fn storage_names() {
        assert_eq!(storage_kind("minecraft:trapped_chest"), Some(0));
        assert_eq!(storage_kind("minecraft:red_shulker_box"), Some(2));
        let mut m = HashMap::new();
        remember(&mut m, (1, 2, 3), 0);
        forget_if_changed(&mut m, (1, 2, 3), None);
        assert!(m.is_empty());
        assert_eq!(storage_kind("minecraft:stone"), None);
    }
    #[test]
    fn center_projects_to_middle() {
        let (x, y) = project([0.0, 0.0, 0.0], 0.0, 0.0, 70.0, 800.0, 600.0, [0.0, 0.0, 5.0]).unwrap();
        assert!((x - 400.0).abs() < 0.1 && (y - 300.0).abs() < 0.1);
        assert!(project([0.0; 3], 0.0, 0.0, 70.0, 800.0, 600.0, [0.0, 0.0, -5.0]).is_none());
    }
    #[test]
    fn freecam_moves_forward() {
        let p = freecam_step([0.0; 3], 0.0, 0.0, [true, false, false, false, false, false], 10.0, 1.0);
        assert!((p[2] - 10.0).abs() < 1e-9);
    }
    #[test]
    fn orbit_is_behind() {
        let p = orbit_position([0.0; 3], 0.0, 0.0, 4.0);
        assert!((p[2] + 4.0).abs() < 1e-9);
    }
    #[test]
    fn vanish_reports_leave_and_return() {
        let (mut k, mut g) = (HashMap::new(), HashMap::new());
        vanish_update(&mut k, &[(1, "A".into())], 0, &mut g, 100);
        assert_eq!(vanish_update(&mut k, &[], 10, &mut g, 100).len(), 1);
        assert_eq!(vanish_update(&mut k, &[(1, "A".into())], 50, &mut g, 100).len(), 1);
    }
    #[test]
    fn sus_threshold() {
        let mut m = HashMap::new();
        m.insert((0, 0), 50);
        m.insert((1, 1), 5);
        assert_eq!(sus_flagged(&m, 40).len(), 1);
    }
}
