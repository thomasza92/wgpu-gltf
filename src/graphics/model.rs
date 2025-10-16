use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::{util::DeviceExt, BindGroup, BindGroupLayout, Buffer, VertexAttribute, VertexBufferLayout};

#[repr(C)]
#[derive(Copy, Clone, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
}
impl Vertex {
    pub fn layout() -> VertexBufferLayout<'static> {
        const ATTRS: &[VertexAttribute] = &wgpu::vertex_attr_array![
            0 => Float32x3, // pos
            1 => Float32x3, // nrm
            2 => Float32x2  // uv
        ];
        VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: ATTRS,
        }
    }
}

#[derive(Debug)]
pub struct GpuMesh {
    pub vbuf: wgpu::Buffer,
    pub ibuf: wgpu::Buffer,
    pub index_count: u32,
    pub material_id: usize,
}

#[derive(Debug)]
pub struct Material {
    pub bind_group: wgpu::BindGroup,
}

#[derive(Debug)]
pub struct Model {
    pub meshes: Vec<GpuMesh>,
    pub materials: Vec<Material>,
    pub recommended_xform: glam::Mat4,
}

pub fn create_model_ubo(device: &wgpu::Device, layout: &BindGroupLayout, model: Mat4) -> (Buffer, BindGroup) {
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("model_ubo"),
        contents: bytemuck::cast_slice(&[model.to_cols_array()]),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("model_bg"),
        layout,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
    });
    (buf, bg)
}
