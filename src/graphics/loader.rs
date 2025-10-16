use std::path::Path;
use anyhow::Result;
use wgpu::{util::{DeviceExt, TextureDataOrder}, BindGroupLayout, Queue, SamplerDescriptor, TextureFormat, TextureUsages, TextureViewDescriptor, TextureDescriptor, TextureDimension, BindGroupEntry, BindingResource};
use crate::graphics::model::{GpuMesh, Material, Model, Vertex};

pub async fn load_gltf_model(
    device: &wgpu::Device,
    queue: &Queue,
    material_bgl: &BindGroupLayout,
    path: &Path,
) -> Result<Model> {
    let (doc, buffers, images) = gltf::import(path)?;
    let mut materials = Vec::<Material>::new();
    if doc.materials().len() == 0 {
        materials.push(make_white_material(device, material_bgl));
    } else {
        for m in doc.materials() {
            let pbr = m.pbr_metallic_roughness();
            let img = pbr
                .base_color_texture()
                .and_then(|t| images.get(t.texture().source().index()));
            materials.push(make_texture_material(device, queue, material_bgl, img));
        }
    }
    let mut meshes = Vec::<GpuMesh>::new();
    let mut min_v = glam::vec3(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max_v = glam::vec3(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);

    for scene in doc.scenes() {
        for node in scene.nodes() {
            if let Some(mesh) = node.mesh() {
                for prim in mesh.primitives() {
                    use gltf::mesh::Mode;
                    assert!(matches!(prim.mode(), Mode::Triangles), "Only triangles supported");

                    let reader = prim.reader(|buf| Some(&buffers[buf.index()].0));

                    let positions = reader.read_positions().expect("POSITION missing")
                        .map(|p| [p[0], p[1], p[2]]);
                    let normals = reader.read_normals()
                        .map(|it| it.map(|n| [n[0], n[1], n[2]]));
                    let uvs = reader.read_tex_coords(0)
                        .map(|tc| tc.into_f32().map(|t| [t[0], t[1]]));

                    let mut verts = Vec::<Vertex>::new();
                    match (normals, uvs) {
                        (Some(ns), Some(uvs)) => for ((p, n), uv) in positions.zip(ns).zip(uvs) {
                            min_v = min_v.min(glam::Vec3::from(p));
                            max_v = max_v.max(glam::Vec3::from(p));
                            verts.push(Vertex { pos: p, nrm: n, uv });
                        },
                        (Some(ns), None) => for (p, n) in positions.zip(ns) {
                            min_v = min_v.min(glam::Vec3::from(p));
                            max_v = max_v.max(glam::Vec3::from(p));
                            verts.push(Vertex { pos: p, nrm: n, uv: [0.0, 0.0] });
                        },
                        (None, Some(uvs)) => for (p, uv) in positions.zip(uvs) {
                            min_v = min_v.min(glam::Vec3::from(p));
                            max_v = max_v.max(glam::Vec3::from(p));
                            verts.push(Vertex { pos: p, nrm: [0.0, 1.0, 0.0], uv });
                        },
                        (None, None) => for p in positions {
                            min_v = min_v.min(glam::Vec3::from(p));
                            max_v = max_v.max(glam::Vec3::from(p));
                            verts.push(Vertex { pos: p, nrm: [0.0, 1.0, 0.0], uv: [0.0, 0.0] });
                        },
                    }

                    let indices: Vec<u32> = reader
                        .read_indices()
                        .map(|r| r.into_u32().collect())
                        .unwrap_or_else(|| (0..verts.len() as u32).collect());

                    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("mesh_vbuf"),
                        contents: bytemuck::cast_slice(&verts),
                        usage: wgpu::BufferUsages::VERTEX,
                    });
                    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("mesh_ibuf"),
                        contents: bytemuck::cast_slice(&indices),
                        usage: wgpu::BufferUsages::INDEX,
                    });

                    let mat_ix = prim.material().index().unwrap_or(0);
                    meshes.push(GpuMesh {
                        vbuf,
                        ibuf,
                        index_count: indices.len() as u32,
                        material_id: mat_ix,
                    });
                }
            }
        }
    }

    let center = (min_v + max_v) * 0.5;
    let extent = max_v - min_v;
    let max_dim = extent.max_element().max(1e-5);
    let scale   = 1.0 / max_dim;
    let recommended_xform =
        glam::Mat4::from_scale(glam::Vec3::splat(scale * 2.0))
        * glam::Mat4::from_translation(-center);

    Ok(Model { meshes, materials, recommended_xform })
}

fn make_white_material(device: &wgpu::Device, material_bgl: &BindGroupLayout) -> Material {
    let rgba = image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    make_texture_material_impl(device, None, Some(rgba), material_bgl)
}

fn make_texture_material(
    device: &wgpu::Device,
    queue: &Queue,
    material_bgl: &BindGroupLayout,
    img: Option<&gltf::image::Data>,
) -> Material {
    let rgba_img = if let Some(g) = img {
        let (w, h) = (g.width, g.height);
        match g.format {
            gltf::image::Format::R8G8B8A8 => {
                image::RgbaImage::from_raw(w, h, g.pixels.clone())
                    .unwrap_or_else(|| image::RgbaImage::from_pixel(1, 1, image::Rgba([255,255,255,255])))
            }
            gltf::image::Format::R8G8B8 => {
                let mut out = Vec::with_capacity((w * h * 4) as usize);
                for c in g.pixels.chunks_exact(3) {
                    out.extend_from_slice(&[c[0], c[1], c[2], 255]);
                }
                image::RgbaImage::from_raw(w, h, out)
                    .unwrap_or_else(|| image::RgbaImage::from_pixel(1, 1, image::Rgba([255,255,255,255])))
            }
            _ => image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255])),
        }
    } else {
        image::RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 255]))
    };

    let (w, h) = (rgba_img.width(), rgba_img.height());

    let tex = device.create_texture_with_data(
        queue,
        &TextureDescriptor {
            label: Some("baseColorTex"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        TextureDataOrder::LayerMajor,
        rgba_img.as_raw(),
    );

    let view = tex.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor {
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        ..Default::default()
    });

    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material_bg"),
        layout: material_bgl,
        entries: &[
            BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&view) },
            BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&sampler) },
        ],
    });

    Material { bind_group: bg }
}

fn make_texture_material_impl(
    device: &wgpu::Device,
    _queue: Option<&Queue>,
    rgba: Option<image::RgbaImage>,
    material_bgl: &BindGroupLayout,
) -> Material {
    let img = rgba.unwrap_or_else(|| image::RgbaImage::from_pixel(1, 1, image::Rgba([255,255,255,255])));
    let tex = device.create_texture(&TextureDescriptor {
        label: Some("baseColorTex"),
        size: wgpu::Extent3d { width: img.width(), height: img.height(), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: TextureFormat::Rgba8UnormSrgb,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = tex.create_view(&TextureViewDescriptor::default());
    let sampler = device.create_sampler(&SamplerDescriptor::default());
    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material_bg"),
        layout: material_bgl,
        entries: &[
            BindGroupEntry { binding: 0, resource: BindingResource::TextureView(&view) },
            BindGroupEntry { binding: 1, resource: BindingResource::Sampler(&sampler) },
        ],
    });
    Material { bind_group: bg }
}
