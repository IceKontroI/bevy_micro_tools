@group(0) @binding(0)
var<uniform> u: Uniform;

struct Uniform {
    quad: array<vec4<f32>, 4>,
    brush: u32,
}

@vertex
fn vertex(@builtin(vertex_index) corner: u32) -> @builtin(position) vec4<f32> {
    return u.quad[corner];
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(f32(u.brush));
}
