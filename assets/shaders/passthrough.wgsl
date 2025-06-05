@group(0) @binding(0)
var draw_canvas: texture_2d<f32>;

@vertex
fn vertex(@builtin(vertex_index) corner: u32) -> @builtin(position) vec4<f32> {
    switch corner {
        case 0u: { return vec4<f32>(-1.0, -1.0, 0.0, 1.0); } // bottom-left
        case 1u: { return vec4<f32>( 1.0, -1.0, 0.0, 1.0); } // bottom-right
        case 2u: { return vec4<f32>(-1.0,  1.0, 0.0, 1.0); } // top-left
        default: { return vec4<f32>( 1.0,  1.0, 0.0, 1.0); } // top-right
    };
}

@fragment
fn fragment(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(draw_canvas, vec2<u32>(position.xy), 0);
}
