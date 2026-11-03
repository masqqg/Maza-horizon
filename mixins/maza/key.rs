fn maza_key(key: &str, pressed: bool) {
    use crate::game::maza as m;
    let k = key.to_ascii_lowercase();
    let k = k.rsplit('.').next().unwrap_or("");
    let i = match k { "w" => 0, "a" => 1, "s" => 2, "d" => 3, "space" => 4, "left_shift" | "lshift" | "shift" => 5, _ => return };
    m::with(|s| s.keys[i] = pressed);
}
