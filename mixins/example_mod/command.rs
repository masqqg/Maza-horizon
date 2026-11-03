fn maza_command(game: &mut crate::game::Game<'_>, command: &str, ran: &mut bool) {
    use crate::game::maza as m;
    let mut it = command.trim_start_matches('/').split_whitespace();
    if it.next() != Some("maza") { return; }
    *ran = true;
    let Some(arg) = it.next() else {
        game.native_chat("/maza storage | sus | vanish | freecam | freelook");
        return;
    };
    let arg = arg.to_ascii_lowercase();
    if arg == "storage" {
        if let Some(sub) = it.next() {
            let sub = sub.to_ascii_lowercase();
            if sub == "tracers" {
                let on = m::with(|s| { s.st_tracers = !s.st_tracers; s.st_tracers });
                game.native_chat(&format!("[Maza] Storage tracers: {}", if on { "ON" } else { "OFF" }));
            } else if let Some(i) = m::STORAGE_KEYS.iter().position(|k| *k == sub) {
                let on = m::with(|s| { s.st_flip[i] = !s.st_flip[i]; m::type_enabled(s, i) });
                game.native_chat(&format!("[Maza] {}: {}", m::STORAGE_NAMES[i], if on { "ON" } else { "OFF" }));
            } else {
                game.native_chat("/maza storage [chest|ender|shulker|spawner|furnace|barrel|enchanting|hopper|tracers]");
            }
            return;
        }
    }
    if arg == "sus" || arg == "bsc" {
        if let Some(sub) = it.next() {
            let sub = sub.to_ascii_lowercase();
            let val: Option<i32> = it.next().and_then(|v| v.parse().ok());
            let msg = m::with(|s| {
                m::ensure_defaults(s);
                match (arg.as_str(), sub.as_str(), val) {
                    ("sus", "anbs", _) => { s.sus_anbs = !s.sus_anbs; s.heat = m::Heat { radius: s.sus_radius, ..Default::default() }; s.scanner.last.clear(); format!("Sus method: {}", if s.sus_anbs { "ANBS+ Scan" } else { "Classic" }) }
                    ("sus", "sens", Some(v)) => { s.sus_sens = v.clamp(1, 20); format!("Sus sensitivity: {}", s.sus_sens) }
                    ("sus", "radius", Some(v)) => { s.sus_radius = v.clamp(2, 16); s.heat = m::Heat { radius: s.sus_radius, ..Default::default() }; s.scanner.last.clear(); format!("Sus radius: {}", s.sus_radius) }
                    ("bsc", "sens", Some(v)) | ("bsc", "sim", Some(v)) => {
                        if sub == "sens" { s.bsc_sens = v.clamp(1, 500); } else { s.bsc_sim = v.clamp(2, 16); }
                        s.bsc = Some(m::bsc::BetterSusEngine::new(m::bsc::BetterSusConfig { simulation_distance: s.bsc_sim, min_flag: s.bsc_sens, hole_filter: false, hole_distance: 2 }));
                        s.scanner.last.clear();
                        format!("BetterSus: sens {} sim {}", s.bsc_sens, s.bsc_sim)
                    }
                    _ => "/maza sus anbs|sens N|radius N  ve  /maza bsc sens N|sim N".to_string(),
                }
            });
            game.native_chat(&format!("[Maza] {msg}"));
            return;
        }
    }
    match m::toggle(&arg) {
        Some((label, on)) => {
            if on && (label == "Freecam" || label == "FreeLook") {
                if let Some(p) = game.native_player() {
                    m::with(|s| {
                        s.cam = [p.x, p.y + 1.62, p.z];
                        s.cam_yaw = p.yaw;
                        s.cam_pitch = p.pitch;
                        if label == "Freecam" { s.freelook = false; } else { s.freecam = false; }
                    });
                }
            }
            game.native_chat(&format!("[Maza] {label}: {}", if on { "ON" } else { "OFF" }));
        }
        None => game.native_chat("[Maza] bilinmeyen modül"),
    }
}
