mod pipeline;
mod depth;
mod camera;
mod model;
mod loader;

use std::path::Path;

use winit::{
    dpi::PhysicalSize,
    event::{WindowEvent, MouseButton, ElementState, MouseScrollDelta, DeviceEvent},
    event_loop::EventLoopProxy,
    window::Window,
};

use wgpu::{
    Adapter, BindGroup, Buffer, Color, CommandEncoderDescriptor, Device, ExperimentalFeatures, Features, Instance, Limits, LoadOp, MemoryHints, Operations, PowerPreference, Queue, RenderPassColorAttachment,
    RenderPassDescriptor, RenderPipeline, RequestAdapterOptions, StoreOp, Surface,
    SurfaceConfiguration, Texture, TextureView, TextureViewDescriptor,
};

pub type Rc<T> = std::sync::Arc<T>;

use camera::{make_camera, update_camera_buffer};
use depth::create_depth;
use loader::load_gltf_model;
use model::{create_model_ubo, Model};
use pipeline::{create_bind_group_layouts, create_pipeline};

pub async fn create_graphics(window: Rc<Window>, proxy: EventLoopProxy<Graphics>) {
    let instance = Instance::default();
    let surface = instance.create_surface(std::sync::Arc::clone(&window)).unwrap();
    let adapter = instance
        .request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        })
        .await
        .expect("Could not get an adapter (GPU).");

    let (device, queue) = adapter
        .request_device(
            &wgpu::DeviceDescriptor {
                label: None,
                required_features: Features::empty(),
                required_limits: Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits()),
                memory_hints: MemoryHints::Performance,
                trace: Default::default(),
                experimental_features: ExperimentalFeatures::disabled(),
            },
        )
        .await
        .expect("Failed to get device");

    let size = window.inner_size();
    let width = size.width.max(1);
    let height = size.height.max(1);
    let surface_config = surface.get_default_config(&adapter, width, height).unwrap();
    surface.configure(&device, &surface_config);
    let (depth_view, depth_tex) = create_depth(&device, surface_config.width, surface_config.height);
    let layouts = create_bind_group_layouts(&device);
    let (render_pipeline, camera_bg, camera_buf, model_bgl) =
        create_pipeline(&device, surface_config.format, &layouts);
    let cam = make_camera(surface_config.width, surface_config.height);
    queue.write_buffer(&camera_buf, 0, bytemuck::cast_slice(&[cam.view_proj.to_cols_array()]));

    let model = load_gltf_model(
        &device,
        &queue,
        &layouts.material_bgl,
        Path::new("assets/BoomBox.glb"),
    )
    .await
    .expect("Failed to load glTF");

    let (model_buf, model_bg) = create_model_ubo(&device, &model_bgl, model.recommended_xform);

    let yaw = 0.6_f32;
    let pitch = 0.5_f32;
    let radius = 3.0_f32;
    let target = glam::vec3(0.0, 0.5, 0.0);

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
        yaw,
        pitch,
        radius,
        target,
        rotating: false,
        last_cursor: glam::vec2(0.0, 0.0),
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
    yaw: f32,
    pitch: f32,
    radius: f32,
    target: glam::Vec3,
    rotating: bool,
    last_cursor: glam::Vec2,
}

impl Graphics {
    pub fn request_redraw(&self) {
        self.window.request_redraw();
    }

    pub fn resize(&mut self, new_size: PhysicalSize<u32>) {
        self.surface_config.width = new_size.width.max(1);
        self.surface_config.height = new_size.height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
        let (dv, dt) = create_depth(&self.device, self.surface_config.width, self.surface_config.height);
        self.depth_view = dv;
        self._depth_tex = dt;
        update_camera_buffer(&self.queue, &self.camera_buf, self.surface_config.width, self.surface_config.height,
                             self.yaw, self.pitch, self.radius, self.target);
    }

    pub fn draw(&mut self) {
        update_camera_buffer(&self.queue, &self.camera_buf, self.surface_config.width, self.surface_config.height,
                             self.yaw, self.pitch, self.radius, self.target);

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
                    depth_ops: Some(Operations { load: LoadOp::Clear(1.0), store: StoreOp::Store }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            r_pass.set_pipeline(&self.render_pipeline);
            r_pass.set_bind_group(0, &self.camera_bg, &[]);
            r_pass.set_bind_group(1, &self.model_bg, &[]);

            for mesh in &self.model.meshes {
                let mat = &self.model.materials[mesh.material_id.min(self.model.materials.len() - 1)];
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
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    self.rotating = *state == ElementState::Pressed;
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let pos = glam::vec2(position.x as f32, position.y as f32);
                if self.rotating {
                    let delta = (pos - self.last_cursor) * 0.005;
                    self.yaw   -= delta.x;
                    self.pitch -= delta.y;
                }
                self.last_cursor = pos;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let d = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => -*y * 0.25,
                    MouseScrollDelta::PixelDelta(p)    => -(p.y as f32) * 0.001,
                };
                self.radius = (self.radius + d).clamp(0.5, 50.0);
            }
            _ => {}
        }
    }

    pub fn handle_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            if self.rotating {
                self.yaw   -= (*dx as f32) * 0.0025;
                self.pitch -= (*dy as f32) * 0.0025;
            }
        }
    }
}
