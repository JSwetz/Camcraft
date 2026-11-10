// The traced stock composited into the 3D pass (M7.2 B3b). One
// fullscreen triangle; each fragment reads the trace frame's vec4
// (depth t, class, shade, status), rebuilds its hit point along the
// viewport's own ray, and writes that point's projected depth so the
// field's surface sorts against the models and lines like any mesh.
// Provenance: deferred-shading composite of a ray-marched depth image,
// textbook rasterization depth (z/w); our own WGSL.

struct C {
    mvp: mat4x4<f32>,
    eye: vec4<f32>,    // xyz eye, w tan(half vertical fov)
    fwd: vec4<f32>,    // xyz forward, w viewport width (px)
    right: vec4<f32>,  // xyz right, w viewport height (px)
    up: vec4<f32>,     // xyz up, w viewport px per trace px
    frame: vec4<f32>,  // trace width, trace height, ortho half height (0 = perspective)
    // fs_stock's verdict palette (M13.4 colour-vision remap)
    clean: vec4<f32>,
    rest: vec4<f32>,
    gouge: vec4<f32>,
    unjudged: vec4<f32>,
};
@group(0) @binding(0) var<uniform> c: C;
@group(0) @binding(1) var<storage, read> px: array<vec4<f32>>;

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}

struct FOut {
    @location(0) color: vec4<f32>,
    @builtin(frag_depth) depth: f32,
};

@fragment
fn fs_composite(@builtin(position) pos: vec4<f32>) -> FOut {
    let vw = max(c.fwd.w, 1.0);
    let vh = max(c.right.w, 1.0);
    let k = max(c.up.w, 1.0);
    let tw = max(u32(c.frame.x), 1u);
    let th = max(u32(c.frame.y), 1u);
    let tx = min(u32(pos.x / k), tw - 1u);
    let ty = min(u32(pos.y / k), th - 1u);
    let s = px[ty * tw + tx];
    // Only a resolved hit draws; a miss shows what is behind, a budget
    // bail is counted by the status line, never painted as material.
    if (s.w != 1.0) {
        discard;
    }
    let u = 2.0 * pos.x / vw - 1.0;
    let v = 1.0 - 2.0 * pos.y / vh;
    // PreviewCamera::ray: the eye's cone, or (#609) in orthographic a
    // parallel ray along fwd from the pixel's point on the eye plane.
    let oh = c.frame.z;
    var origin = c.eye.xyz;
    var dir = c.fwd.xyz;
    if (oh > 0.0) {
        origin = c.eye.xyz + c.right.xyz * (u * oh * (vw / vh)) + c.up.xyz * (v * oh);
    } else {
        let a = c.eye.w * (vw / vh);
        dir = normalize(c.fwd.xyz + c.right.xyz * (u * a) + c.up.xyz * (v * c.eye.w));
    }
    let hit = origin + dir * s.x;
    let clip = c.mvp * vec4<f32>(hit, 1.0);
    if (clip.w <= 0.0) {
        discard;
    }
    let d = clip.z / clip.w;
    if (d < 0.0 || d > 1.0) {
        discard;
    }
    // fs_stock's verdict palette, opaque: steel clean, amber rest, red
    // gouge, flat gray where no verdict was rendered (never green).
    var tint = c.clean.xyz;
    switch (i32(s.y + 0.5)) {
        case 1: { tint = c.rest.xyz; }
        case 2: { tint = c.gouge.xyz; }
        case 3: { tint = c.unjudged.xyz; }
        default: {}
    }
    let light = 0.2 + 0.8 * clamp(s.z, 0.0, 1.0);
    var o: FOut;
    o.color = vec4<f32>(tint * light, 1.0);
    o.depth = d;
    return o;
}
