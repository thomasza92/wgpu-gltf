struct Camera {
  view_proj : mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> camera : Camera;

struct ModelXform {
  model : mat4x4<f32>,
}
@group(1) @binding(0) var<uniform> model_xform : ModelXform;

@group(2) @binding(0) var texBase : texture_2d<f32>;
@group(2) @binding(1) var samp    : sampler;

struct VsIn {
  @location(0) pos : vec3<f32>,
  @location(1) nrm : vec3<f32>,
  @location(2) uv  : vec2<f32>,
}
struct VsOut {
  @builtin(position) pos : vec4<f32>,
  @location(0) nrm : vec3<f32>,
  @location(1) uv  : vec2<f32>,
}

@vertex
fn vs_main(in: VsIn) -> VsOut {
  var out: VsOut;
  let world = model_xform.model * vec4<f32>(in.pos, 1.0);
  out.pos = camera.view_proj * world;
  out.nrm = normalize((model_xform.model * vec4<f32>(in.nrm, 0.0)).xyz);
  out.uv = in.uv;
  return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
  let base = textureSample(texBase, samp, in.uv);
  let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
  let lambert = max(dot(normalize(in.nrm), light_dir), 0.1);
  return vec4<f32>(base.rgb * lambert, base.a);
}
