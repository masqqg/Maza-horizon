fn maza_hud(game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch) {
    use crate::game::maza as m;
    let Some(p) = game.native_player() else { return };
    let (w, h) = (batch.width as f32, batch.height as f32);
    let dim = format!("{:?}", p.dimension);
    let (cam, yaw, pitch) = m::with(|s| {
        if s.freecam || s.freelook { (s.cam, s.cam_yaw, s.cam_pitch) } else { ([p.x, p.y + 1.62, p.z], p.yaw, p.pitch) }
    });
    let (storage, tracers, sus) = m::with(|s| (s.storage, s.st_tracers, s.sus));
    if storage {
        let mut list: Vec<((i32, i32, i32), u8, bool)> = m::with(|s| {
            s.det.get(&dim).map(|d| d.iter().map(|(k, v)| (*k, *v, m::type_enabled(s, *v as usize))).collect()).unwrap_or_default()
        });
        list.retain(|e| e.2);
        list.sort_by(|a, b| {
            let da = (f64::from(a.0.0) - cam[0]).powi(2) + (f64::from(a.0.2) - cam[2]).powi(2);
            let db = (f64::from(b.0.0) - cam[0]).powi(2) + (f64::from(b.0.2) - cam[2]).powi(2);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
        for ((x, y, z), kind, _) in list.into_iter().take(300) {
            let color = m::STORAGE_COLORS[kind as usize];
            let c = [f64::from(x) + 0.5, f64::from(y) + 0.5, f64::from(z) + 0.5];
            if let Some((sx, sy)) = m::project(cam, yaw, pitch, 70.0, w, h, c) {
                if sx < 0.0 || sy < 0.0 || sx > w || sy > h { continue; }
                game.native_fill(batch, [sx - 4.0, sy - 4.0, 8.0, 8.0], (color & 0x00FF_FFFF) | 0x66000000);
                game.native_fill(batch, [sx - 4.0, sy - 4.0, 8.0, 1.0], color);
                game.native_fill(batch, [sx - 4.0, sy + 3.0, 8.0, 1.0], color);
                game.native_fill(batch, [sx - 4.0, sy - 4.0, 1.0, 8.0], color);
                game.native_fill(batch, [sx + 3.0, sy - 4.0, 1.0, 8.0], color);
                let label = m::STORAGE_NAMES[kind as usize];
                game.native_text(batch, label, [sx + 6.0, sy - 4.0], color & 0x00FF_FFFF, true);
                if tracers {
                    let (x0, y0) = (w / 2.0, h);
                    for i in 0..24 {
                        let t = i as f32 / 24.0;
                        game.native_fill(batch, [x0 + (sx - x0) * t, y0 + (sy - y0) * t, 2.0, 2.0], color);
                    }
                }
            }
        }
    }
    let (bsc_on, bsc_list, sus_list) = m::with(|s| (s.bsc_on, s.bsc_flagged.clone(), s.sus_flagged.clone()));
    let mut row = 0.0f32;
    let mut mark = |game: &mut crate::game::Game<'_>, batch: &mut horizon_client::hud::HudBatch, tag: &str, cx: i32, cz: i32, extra: String, color: u32| {
        let (wx, wz) = (f64::from(cx * 16 + 8), f64::from(cz * 16 + 8));
        game.native_text(batch, &format!("{tag} {wx:.0}, {wz:.0}{extra}"), [4.0, 4.0 + row], color, true);
        row += 10.0;
        if let Some((sx, sy)) = m::project(cam, yaw, pitch, 70.0, w, h, [wx, cam[1], wz]) {
            if sx >= 0.0 && sy >= 0.0 && sx <= w && sy <= h {
                game.native_fill(batch, [sx - 3.0, sy - 3.0, 6.0, 6.0], 0xFF000000 | color);
                game.native_text(batch, tag, [sx + 5.0, sy - 4.0], color, true);
            }
        }
    };
    if sus {
        for ((cx, cz), v) in sus_list.iter().take(8) { mark(game, batch, "SUS", *cx, *cz, format!("  ({v})"), 0xFFB45C); }
    }
    if bsc_on {
        for (cx, cz) in bsc_list.iter().take(8) { mark(game, batch, "BSC", *cx, *cz, String::new(), 0xFF5C5C); }
    }
}
