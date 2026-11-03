fn maza_tick(game: &mut crate::game::Game<'_>) {
    use crate::game::maza as m;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;
    let Some(p) = game.native_player() else { return };
    m::with(|s| { s.tick += 1; m::ensure_defaults(s); });
    let (storage, sus, bsc_on, vanish, freecam, tick) = m::with(|s| (s.storage, s.sus, s.bsc_on, s.vanish, s.freecam, s.tick));
    let dim = format!("{:?}", p.dimension);
    let (pcx, pcz) = ((p.x.floor() as i32) >> 4, (p.z.floor() as i32) >> 4);
    let (bottom, top) = (game.world.bottom_y(), game.world.top_y() - 1);

    if storage || sus || bsc_on {
        // Block view over the game's world (block info cached per state).
        let cache: RefCell<HashMap<_, Rc<m::Info>>> = RefCell::new(HashMap::new());
        let g: &crate::game::Game<'_> = &*game;
        let info = |x: i32, y: i32, z: i32| -> Rc<m::Info> {
            let st = g.world.block(x, y, z);
            if let Some(i) = cache.borrow().get(&st) { return i.clone(); }
            let i = Rc::new(match g.pack.assets.registry.state(st) {
                Some(r) => m::Info::new(&r.name, r.properties.iter().map(|(k, v)| (k.clone(), v.clone())).collect()),
                None => m::Info::new(&g.res.facts().get(st).name, Vec::new()),
            });
            cache.borrow_mut().insert(st, i.clone());
            i
        };
        let light = |x: i32, y: i32, z: i32| -> u8 { let l = g.world.light(x, y, z); (l & 15).max(l >> 4) };
        let block_light = |x: i32, y: i32, z: i32| -> u8 { g.world.light(x, y, z) & 15 };
        let view = m::FnView { info: &info, light: &light, block_light: &block_light };

        let next = m::with(|s| s.scanner.next(pcx, pcz, 12, tick, |cx, cz| g.world.loaded(cx, cz)));
        if let Some((cx, cz)) = next {
            let anbs = m::with(|s| s.sus_anbs);
            let out = m::scan_chunk(&view, cx, cz, bottom, top, bsc_on, sus && !anbs, storage);
            let glow = if sus && anbs { m::glow_scan(&view, cx, cz) / 12 } else { 0 };
            m::with(|s| {
                if storage {
                    let d = s.det.entry(dim.clone()).or_default();
                    d.retain(|k, _| (k.0 >> 4, k.2 >> 4) != (cx, cz));
                    for (pos, k) in &out.storage { m::remember(d, *pos, *k); }
                    s.st_scanned.insert((dim.clone(), cx, cz));
                }
                if sus {
                    let mut grown = out.grown;
                    if anbs { grown[0] = glow; }
                    s.heat.update(cx, cz, grown, out.ungrown);
                }
                if bsc_on {
                    if let Some(e) = s.bsc.as_mut() {
                        e.observe(m::bsc::ChunkObservation { x: cx, z: cz, overdue_count: out.bs_count, has_hole: false });
                    }
                }
            });
        }
        if tick % 20 == 0 {
            m::with(|s| {
                if sus { s.sus_flagged = s.heat.visible(s.sus_sens, |cx, cz| g.world.loaded(cx, cz)); }
                if bsc_on { if let Some(e) = s.bsc.as_ref() { s.bsc_flagged = e.flagged_chunks(); } }
            });
        }
        if tick % 100 == 0 {
            m::with(|s| { for (cx, cz) in s.scanner.forget_far(pcx, pcz, 16) { s.heat.withdraw(cx, cz); } });
        }
        // Block changes: re-check a few remembered storage blocks each 5 ticks.
        if storage && tick % 5 == 0 {
            let list: Vec<(i32, i32, i32)> = m::with(|s| {
                let all: Vec<_> = s.det.get(&dim).map(|d| d.keys().copied().collect()).unwrap_or_default();
                let n = all.len();
                if n == 0 { return Vec::new(); }
                let start = s.st_check % n;
                s.st_check = start + 64;
                all.into_iter().cycle().skip(start).take(64.min(n)).collect()
            });
            for pos in list {
                if !g.world.loaded(pos.0 >> 4, pos.2 >> 4) { continue; }
                let now = usize::try_from(info(pos.0, pos.1, pos.2).storage).ok();
                m::with(|s| { if let Some(d) = s.det.get_mut(&dim) { m::forget_if_changed(d, pos, now); } });
            }
        }
    }

    if vanish && tick % 20 == 0 {
        if let Some((players, _, _)) = game.remote_tab_players() {
            let now: Vec<(u128, String)> = players.iter().map(|t| (t.uuid, t.name.clone())).collect();
            let msgs = m::with(|s| {
                let mut gone = std::mem::take(&mut s.gone_at);
                let r = m::vanish_update(&mut s.known_tab, &now, s.tick, &mut gone, 6000);
                s.gone_at = gone;
                r
            });
            for line in msgs { game.native_chat(&format!("[Maza AntiVanish] {line}")); }
        }
    }
    if freecam {
        m::with(|s| { s.cam = m::freecam_step(s.cam, s.cam_yaw, s.cam_pitch, s.keys, 25.0, 0.05); });
    }
                  }
