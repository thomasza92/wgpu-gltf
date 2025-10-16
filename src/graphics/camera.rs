use glam::{Mat4, Vec3};
use wgpu::{Queue, Buffer};

pub struct CameraData {
    pub view_proj: Mat4,
}

pub fn make_camera(w: u32, h: u32) -> CameraData {
    let eye = Vec3::new(2.0, 2.0, 3.5);
    let tgt = Vec3::new(0.0, 0.5, 0.0);
    let up = Vec3::Y;
    let view = Mat4::look_at_rh(eye, tgt, up);
    let aspect = (w as f32).max(1.0) / (h as f32).max(1.0);
    let proj = Mat4::perspective_rh_gl(45f32.to_radians(), aspect, 0.1, 100.0);
    CameraData { view_proj: proj * view }
}

pub fn orbit_eye(yaw: f32, pitch: f32, radius: f32, target: Vec3) -> Vec3 {
    let pitch = pitch.clamp(-1.53, 1.53);
    let cp = pitch.cos();
    let sp = pitch.sin();
    let cy = yaw.cos();
    let sy = yaw.sin();
    let dir = glam::vec3(cy * cp, sp, -sy * cp);
    target + dir * radius
}

pub fn update_camera_buffer(
    queue: &Queue,
    camera_buf: &Buffer,
    w: u32,
    h: u32,
    yaw: f32,
    pitch: f32,
    radius: f32,
    target: Vec3,
) {
    let eye = orbit_eye(yaw, pitch, radius, target);
    let view = Mat4::look_at_rh(eye, target, Vec3::Y);
    let aspect = (w as f32).max(1.0) / (h as f32).max(1.0);
    let proj  = Mat4::perspective_rh_gl(45f32.to_radians(), aspect, 0.1, 100.0);
    let vp = (proj * view).to_cols_array();
    queue.write_buffer(camera_buf, 0, bytemuck::cast_slice(&[vp]));
}
