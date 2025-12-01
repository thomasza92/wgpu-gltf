mod camera;
mod depth;
mod loader;
mod model;
mod pipeline;

use std::{path::Path, time::Instant};
use winit::{
    dpi::PhysicalSize,
    event::{DeviceEvent, ElementState, KeyEvent, MouseButton, WindowEvent},
    event_loop::EventLoopProxy,
    keyboard::KeyCode,
    window::Window,
};

use wgpu::{
    Adapter, BindGroup, Buffer, Color, CommandEncoderDescriptor, Device, ExperimentalFeatures,
    Features, Instance, Limits, LoadOp, MemoryHints, Operations, PowerPreference, Queue,
    RenderPassColorAttachment, RenderPassDescriptor, RenderPipeline, RequestAdapterOptions,
    StoreOp, Surface, SurfaceConfiguration, Texture, TextureView, TextureViewDescriptor,
};

pub type Rc<T> = std::sync::Arc<T>;

use camera::{forward_from_yaw_pitch, make_camera, update_camera_buffer};
use depth::create_depth;
use loader::load_gltf_model;
use model::{Model, create_model_ubo};
use pipeline::{create_bind_group_layouts, create_pipeline};

const CAMERA_SPEED: f32 = 3.0;

pub async fn create_graphics(window: Rc<Window>, proxy: EventLoopProxy<Graphics>) {
    let instance = Instance::default();
    let surface = instance
        .create_surface(std::sync::Arc::clone(&window))
        .unwrap();

    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Could not get an adapter (GPU).");

    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: None,
            required_features: Features::empty(),
            required_limits: Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
            memory_hints: MemoryHints::Performance,
            trace: Default::default(),
            experimental_features: ExperimentalFeatures::disabled(),
        })
        .await
        .expect("Failed to get device");

    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    let surface_config = surface.get_default_config(&adapter, width, height).unwrap();
    surface.configure(&device, &surface_config);

    let (depth_view, depth_tex) =
        create_depth(&device, surface_config.width, surface_config.height);
    let layouts = create_bind_group_layouts(&device);

    let (render_pipeline, camera_bg, camera_buf, model_bgl) =
        create_pipeline(&device, surface_config.format, &layouts);

    let cam = make_camera(surface_config.width, surface_config.height);
    queue.write_buffer(
        &camera_buf,
        0,
        bytemuck::cast_slice(&[cam.view_proj.to_cols_array()]),
    );

    let model = load_gltf_model(
        &device,
        &queue,
        &layouts.material_bgl,
        Path::new("assets/BoomBox.glb"),
    )
    .await
    .expect("Failed to load glTF");

    let (model_buf, model_bg) = create_model_ubo(&device, &model_bgl, model.recommended_xform);
    let eye = glam::vec3(2.0, 2.0, 3.5);
    let yaw = 0.6_f32;
    let pitch = 0.5_f32;

    let gfx = Graphics {
        window: window.clone(),
        instance,
        surface,
        surface_config,
        adapter,
        device,
        queue,
        render_pipeline,
        depth_view,
        _depth_tex: depth_tex,
        camera_bg,
        camera_buf,
        model_bg,
        model_buf,
        model,
        eye,
        yaw,
        pitch,
        rotating: false,
        last_cursor: glam::vec2(0.0, 0.0),
        move_forward: false,
        move_back: false,
        move_left: false,
        move_right: false,
        move_up: false,
        move_down: false,
        boost_speed: false,
        last_frame_time: Instant::now(),
    };

    let _ = proxy.send_event(gfx);
}

#[allow(dead_code)]
#[derive(Debug)]
pub struct Graphics {
    pub(crate) window: Rc<Window>,
    instance: Instance,
    surface: Surface<'static>,
    surface_config: SurfaceConfiguration,
    adapter: Adapter,
    device: Device,
    queue: Queue,
    render_pipeline: RenderPipeline,
    depth_view: TextureView,
    _depth_tex: Texture,
    camera_bg: BindGroup,
    camera_buf: Buffer,
    model_bg: BindGroup,
    model_buf: Buffer,
    model: Model,
    eye: glam::Vec3,
    yaw: f32,
    pitch: f32,
    rotating: bool,
    last_cursor: glam::Vec2,
    move_forward: bool,
    move_back: bool,
    move_left: bool,
    move_right: bool,
    move_up: bool,
    move_down: bool,
    boost_speed: bool,
    last_frame_time: Instant,
}

impl Graphics {
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.surface_config.width = new_size.width.max(1);
        self.surface_config.height = new_size.height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
        let (dv, dt) = create_depth(
            &self.device,
            self.surface_config.width,
            self.surface_config.height,
        );
        self.depth_view = dv;
        self._depth_tex = dt;

        update_camera_buffer(
            &self.queue,
            &self.camera_buf,
            self.surface_config.width,
            self.surface_config.height,
            self.eye,
            self.yaw,
            self.pitch,
        );
    }

    pub fn draw(&mut self) {
        let now = Instant::now();
        let mut dt = (now - self.last_frame_time).as_secs_f32();
        self.last_frame_time = now;

        if dt > 0.1 {
            dt = 0.1;
        }

        let forward = forward_from_yaw_pitch(self.yaw, self.pitch);
        let mut flat_forward = glam::vec3(forward.x, 0.0, forward.z);
        let ff_len = flat_forward.length();
        if ff_len > 0.0 {
            flat_forward /= ff_len;
        }

        let mut right = flat_forward.cross(glam::Vec3::Y);
        let r_len = right.length();
        if r_len > 0.0 {
            right /= r_len;
        }

        let mut movement = glam::Vec3::ZERO;

        if self.move_forward {
            movement += flat_forward;
        }
        if self.move_back {
            movement -= flat_forward;
        }
        if self.move_right {
            movement += right;
        }
        if self.move_left {
            movement -= right;
        }

        if self.move_up {
            movement += glam::Vec3::Y;
        }
        if self.move_down {
            movement -= glam::Vec3::Y;
        }

        if movement.length_squared() > 0.0 {
            movement = movement.normalize();

            let mut speed = CAMERA_SPEED;
            if self.boost_speed {
                speed *= 5.0;
            }

            self.eye += movement * speed * dt;
        }

        update_camera_buffer(
            &self.queue,
            &self.camera_buf,
            self.surface_config.width,
            self.surface_config.height,
            self.eye,
            self.yaw,
            self.pitch,
        );

        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire next swap chain texture.");

        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });

        {
            let mut r_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: Operations {
                        load: LoadOp::Clear(Color::BLACK),
                        store: StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(Operations {
                        load: LoadOp::Clear(1.0),
                        store: StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            r_pass.set_pipeline(&self.render_pipeline);
            r_pass.set_bind_group(0, &self.camera_bg, &[]);
            r_pass.set_bind_group(1, &self.model_bg, &[]);

            for mesh in &self.model.meshes {
                let mat =
                    &self.model.materials[mesh.material_id.min(self.model.materials.len() - 1)];
                r_pass.set_bind_group(2, &mat.bind_group, &[]);
                r_pass.set_vertex_buffer(0, mesh.vbuf.slice(..));
                r_pass.set_index_buffer(mesh.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                r_pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();
    }

    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: winit::keyboard::PhysicalKey::Code(code),
                        state,
                        repeat,
                        ..
                    },
                ..
            } => {
                if *repeat {
                    return;
                }

                let pressed = *state == ElementState::Pressed;
                match code {
                    KeyCode::KeyW => self.move_forward = pressed,
                    KeyCode::KeyS => self.move_back = pressed,
                    KeyCode::KeyA => self.move_left = pressed,
                    KeyCode::KeyD => self.move_right = pressed,
                    KeyCode::KeyJ => self.move_up = pressed,
                    KeyCode::KeyK => self.move_down = pressed,
                    KeyCode::ShiftLeft => self.boost_speed = pressed,
                    _ => {}
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    self.rotating = *state == ElementState::Pressed;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pos = glam::vec2(position.x as f32, position.y as f32);
                if self.rotating {
                    let delta = (pos - self.last_cursor) * 0.005;
                    self.yaw -= delta.x;
                    self.pitch -= delta.y;
                }
                self.last_cursor = pos;
            }
            _ => {}
        }
    }

    pub fn handle_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.rotating {
                self.yaw -= (*dx as f32) * 0.0025;
                self.pitch -= (*dy as f32) * 0.0025;
            }
        }
    }
}
