#import bevy_sprite::mesh2d_vertex_output::VertexOutput
#ifdef TONEMAP_IN_SHADER
#import bevy_sprite::mesh2d_view_bindings::view
#import bevy_core_pipeline::tonemapping
#endif
#ifdef SRGB_OUTPUT
#import bevy_render::color_operations::linear_to_srgb
#endif
#ifdef OKLAB_OUTPUT
#import bevy_render::color_operations::linear_rgb_to_oklab
#endif

struct CellMaterial {
    fill: vec4<f32>,
    rim: vec4<f32>,
    // Fill half-size, corner radius, padded quad size, maximum border width.
    shape: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> cell: CellMaterial;

// Signed distance to the fill boundary in world units. Both edges of the
// border use this SAME distance: offsetting it expands the radius as well as
// the sides, so there is no second, mismatched rounded rectangle.
fn rounded_square(p: vec2<f32>, half_size: f32, radius: f32) -> f32 {
    let q = abs(p) - vec2(half_size - radius);
    return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fragment(mesh: VertexOutput) -> @location(0) vec4<f32> {
    let p = (mesh.uv - vec2(0.5)) * cell.shape.z;
    let distance = rounded_square(p, cell.shape.x, cell.shape.y);
    // Euclidean gradient gives the same one-pixel transition on corners and
    // straight sides. fwidth's L1 gradient softens diagonal edges more.
    let pixel = max(length(vec2(dpdx(distance), dpdy(distance))), 0.00001);
    // One physical pixel regardless of zoom. Fade to subpixel coverage when
    // zoomed out rather than letting neighboring borders consume the gutter.
    let width = min(pixel, cell.shape.w);
    let fill_coverage = clamp(0.5 - distance / pixel, 0.0, 1.0);
    let outer_coverage = clamp(0.5 - (distance - width) / pixel, 0.0, 1.0);
    let rim_coverage = max(outer_coverage - fill_coverage, 0.0);

    // Use the same quiet food-dependent border colour as before, but apply
    // it only outside the fill. Coverage-weighted colour keeps the shared
    // antialiased boundary free of a dark seam or a double-blended halo.
    let rim_colour = mix(cell.fill.rgb, cell.rim.rgb, cell.rim.a);
    let colour = (cell.fill.rgb * fill_coverage + rim_colour * rim_coverage)
        / max(outer_coverage, 0.00001);
    var output = vec4(colour, cell.fill.a * outer_coverage);
#ifdef TONEMAP_IN_SHADER
    output = tonemapping::tone_mapping(output, view.color_grading);
#endif
#ifdef SRGB_OUTPUT
    output = vec4(linear_to_srgb(output.rgb), output.a);
#endif
#ifdef OKLAB_OUTPUT
    output = vec4(linear_rgb_to_oklab(output.rgb), output.a);
#endif
    return output;
}
