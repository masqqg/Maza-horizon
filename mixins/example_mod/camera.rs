fn maza_camera(view: Option<(glam::DVec3, f32, f32)>) -> Option<(glam::DVec3, f32, f32)> {
    use crate::game::maza as m;
    let (fc, fl, cam, yaw, pitch) = m::with(|s| (s.freecam, s.freelook, s.cam, s.cam_yaw, s.cam_pitch));
    let (pos, vyaw, vpitch) = view?;
    if fc {
        return Some((glam::DVec3::new(cam[0], cam[1], cam[2]), yaw, pitch));
    }
    if fl {
        let o = m::orbit_position([pos.x, pos.y, pos.z], vyaw, vpitch, 4.0);
        m::with(|s| { s.cam = o; s.cam_yaw = vyaw; s.cam_pitch = vpitch; });
        return Some((glam::DVec3::new(o[0], o[1], o[2]), vyaw, vpitch));
    }
    Some((pos, vyaw, vpitch))
}
