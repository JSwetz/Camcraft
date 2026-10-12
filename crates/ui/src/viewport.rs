//! 3D viewport: indexed triangles with a z-buffer, rendered offscreen and
//! handed to ImGui as an external texture so the view docks like any panel.
//! Provenance: standard orbit camera + z-buffered indexed rendering from
//! public graphics literature.
//!
//! Selection is baked into the vertex stream (an extra float per vertex):
//! `Queue::write_buffer` lands at submit time, so a per-draw uniform tint
//! would silently take the last frame's color - one uniform means one
//! upload per frame and one draw call.

use std::collections::HashMap;

use dear_imgui_wgpu::wgpu;

/// vec3 position + f32 selection flag.
const VERTEX_STRIDE: u64 = 16;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = "
struct U { mvp: mat4x4<f32>, eye: vec3<f32> };
@group(0) @binding(0) var<uniform> u: U;

struct VSOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) sel: f32,
    @location(1) world: vec3<f32>,
};

@vertex
fn vs_main(@location(0) pos: vec3<f32>, @location(1) sel: f32) -> VSOut {
    var out: VSOut;
    out.clip = u.mvp * vec4<f32>(pos, 1.0);
    out.sel = sel;
    out.world = pos;
    return out;
}

// Flat shading: STL/OBJ facets carry no smooth normals, so recover the
// face normal from screen-space derivatives of the world position.
// Key + fill give the broad gradient; a view-dependent specular adds the
// CAD-style sheen that makes facet angles readable at a glance.
@fragment
fn fs_main(in: VSOut) -> @location(0) vec4<f32> {
    let cr = cross(dpdx(in.world), dpdy(in.world));
    let len = length(cr);
    let n = select(vec3<f32>(0.0, 0.0, 1.0), cr / len, len > 1e-9);
    let key_dir = normalize(vec3<f32>(0.45, 0.60, 0.66));
    // abs(): two-sided, so flipped windings never render unlit-black.
    let key = abs(dot(n, key_dir));
    let fill = abs(dot(n, normalize(vec3<f32>(-0.55, 0.25, 0.40))));
    let light = 0.34 + 0.52 * key + 0.18 * fill;
    let to_eye = normalize(u.eye - in.world);
    let halfv = normalize(key_dir + to_eye);
    let spec = pow(max(abs(dot(n, halfv)), 0.0), 24.0) * 0.35 * key;
    let base = vec3<f32>(0.66, 0.70, 0.76) * light + vec3<f32>(spec);
    let hot = vec3<f32>(0.98, 0.72, 0.20) * light + vec3<f32>(spec);
    return vec4<f32>(mix(base, hot, in.sel), 1.0);
}

// Facet edges: dark over the shaded pass, warm when the part is selected.
@fragment
fn fs_edge(in: VSOut) -> @location(0) vec4<f32> {
    let dark = vec3<f32>(0.06, 0.07, 0.09);
    let warm = vec3<f32>(0.55, 0.35, 0.05);
    return vec4<f32>(mix(dark, warm, in.sel), 1.0);
}
";

pub struct View3D {
    pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    index_count: u32,
    edge_buf: wgpu::Buffer,
    edge_count: u32,
    _color_tex: wgpu::Texture,
    color_view: wgpu::TextureView,
    _depth_tex: wgpu::Texture,
    depth_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl View3D {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport3d"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport3d-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport3d-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });

        // mat4x4 (64) + eye vec3 (aligned to 64, padded to the 80-byte
        // uniform stride).
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-uniform"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport3d-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform.as_entire_binding(),
            }],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // Facet edges over the shaded pass. No culling (back-face wire is
        // hidden by depth anyway) and a negative slope bias so the lines
        // win the depth test against the faces they lie on.
        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport3d-lines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: VERTEX_STRIDE,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32x3,
                            offset: 0,
                            shader_location: 0,
                        },
                        wgpu::VertexAttribute {
                            format: wgpu::VertexFormat::Float32,
                            offset: 12,
                            shader_location: 1,
                        },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                // Lines may not carry a depth bias in wgpu 30, so edges win
                // ties against the faces they lie on via LessEqual instead.
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_edge"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        // 4-byte placeholders keep the empty-document path simple.
        let dummy = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: 4,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let vertex_buf = dummy("viewport3d-vbuf", wgpu::BufferUsages::VERTEX);
        let index_buf = dummy("viewport3d-ibuf", wgpu::BufferUsages::INDEX);
        let edge_buf = dummy("viewport3d-ebuf", wgpu::BufferUsages::INDEX);

        let targets = make_targets(device, format, width, height);

        View3D {
            pipeline,
            line_pipeline,
            bind_group,
            uniform,
            vertex_buf,
            index_buf,
            index_count: 0,
            edge_buf,
            edge_count: 0,
            _color_tex: targets.color,
            color_view: targets.color_view,
            _depth_tex: targets.depth,
            depth_view: targets.depth_view,
            width: width.max(1),
            height: height.max(1),
        }
    }

    /// The ImGui-facing color target; re-register/update after resize.
    #[must_use]
    pub fn color_view(&self) -> &wgpu::TextureView {
        &self.color_view
    }

    #[must_use]
    pub const fn extent(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) {
        let (width, height) = (width.max(1), height.max(1));
        if width == self.width && height == self.height {
            return;
        }
        let targets = make_targets(device, format, width, height);
        self._color_tex = targets.color;
        self.color_view = targets.color_view;
        self._depth_tex = targets.depth;
        self.depth_view = targets.depth_view;
        self.width = width;
        self.height = height;
    }

    /// Replace the GPU document. Each part is world-space triangles
    /// (translation applied by the caller) plus a selection flag.
    pub fn set_document(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, parts: &[(&[[f32; 3]], bool)]) {
        let mut verts: Vec<u8> = Vec::new();
        let mut idx: Vec<u32> = Vec::new();
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut vertex_count = 0u32;
        for (tris, selected) in parts {
            let sel: f32 = if *selected { 1.0 } else { 0.0 };
            for v in *tris {
                for c in v {
                    verts.extend_from_slice(&c.to_le_bytes());
                }
                verts.extend_from_slice(&sel.to_le_bytes());
                positions.push(*v);
            }
            let n = tris.len() as u32;
            idx.extend(vertex_count..vertex_count + n);
            vertex_count += n;
        }
        if verts.is_empty() {
            self.index_count = 0; // render() then draws nothing, just clears
            self.edge_count = 0;
            return;
        }
        // CAD-style wire: crease and boundary edges only, not every facet
        // seam - flat runs of triangles must read as one shaded surface.
        let edges = feature_edges(&positions);
        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-vbuf"),
            size: verts.len() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&vertex_buf, 0, &verts);
        let idx_bytes: Vec<u8> = idx.iter().flat_map(|i| i.to_le_bytes()).collect();
        let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-ibuf"),
            size: idx_bytes.len() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&index_buf, 0, &idx_bytes);
        let edge_bytes: Vec<u8> = edges.iter().flat_map(|i| i.to_le_bytes()).collect();
        let edge_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport3d-ebuf"),
            size: edge_bytes.len() as u64,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&edge_buf, 0, &edge_bytes);
        self.vertex_buf = vertex_buf;
        self.index_buf = index_buf;
        self.index_count = idx.len() as u32;
        self.edge_buf = edge_buf;
        self.edge_count = edges.len() as u32;
    }

    /// Encode the 3D pass into the offscreen target. `eye` is the camera
    /// position in world space, for the specular term.
    pub fn render(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        mvp: &[[f32; 4]; 4],
        eye: [f32; 3],
    ) {
        let mut data = [0u8; 80];
        let mut o = 0usize;
        for col in mvp {
            for v in col {
                data[o..o + 4].copy_from_slice(&v.to_le_bytes());
                o += 4;
            }
        }
        for v in eye {
            data[o..o + 4].copy_from_slice(&v.to_le_bytes());
            o += 4;
        }
        queue.write_buffer(&self.uniform, 0, &data);

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("viewport3d-pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.09,
                        g: 0.10,
                        b: 0.12,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        if self.index_count > 0 {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
            if self.edge_count > 0 {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_index_buffer(self.edge_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.edge_count, 0, 0..1);
            }
        }
    }
}

struct Targets {
    color: wgpu::Texture,
    color_view: wgpu::TextureView,
    depth: wgpu::Texture,
    depth_view: wgpu::TextureView,
}

fn make_targets(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Targets {
    let size = wgpu::Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let color = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport3d-color"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let color_view = color.create_view(&wgpu::TextureViewDescriptor::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("viewport3d-depth"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&wgpu::TextureViewDescriptor::default());
    Targets {
        color,
        color_view,
        depth,
        depth_view,
    }
}

/// Orbit camera around a target point, Z-up (the CAM convention).
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: [f64; 3],
    pub yaw: f64,
    pub pitch: f64,
    pub dist: f64,
}

impl Default for Camera {
    fn default() -> Self {
        Camera {
            target: [0.0, 0.0, 0.0],
            yaw: 0.9,
            pitch: 0.5,
            dist: 80.0,
        }
    }
}

impl Camera {
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw -= f64::from(dx) * 0.01;
        self.pitch = (self.pitch + f64::from(dy) * 0.01).clamp(-1.5, 1.5);
        if !self.yaw.is_finite() {
            self.yaw = self.yaw.rem_euclid(std::f64::consts::TAU);
            if !self.yaw.is_finite() {
                self.yaw = 0.0;
            }
        }
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        let scale = self.dist * 0.0015;
        let (rx, ry, _) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        self.target = [
            self.target[0] - f64::from(dx) * scale * rx + f64::from(dy) * scale * ux,
            self.target[1] - f64::from(dx) * scale * ry + f64::from(dy) * scale * uy,
            self.target[2] + f64::from(dy) * scale * uz,
        ];
        if self.target.iter().any(|t| !t.is_finite()) {
            self.target = [0.0, 0.0, 0.0];
        }
    }

    pub fn zoom(&mut self, wheel_steps: f32) {
        // One wheel click ~= 10% dolly; clamped to sane machining distances.
        let factor = (1.0 - f64::from(wheel_steps) * 0.1).clamp(0.5, 2.0);
        self.dist = (self.dist * factor).clamp(0.5, 100_000.0);
        if !self.dist.is_finite() {
            self.dist = 80.0;
        }
    }

    /// Ray through an NDC cursor position (x right, y up, -1..1):
    /// (origin, normalized direction), or None on degenerate math.
    #[must_use]
    pub fn ray_through(&self, ndc: [f32; 2], aspect: f64) -> Option<([f64; 3], [f64; 3])> {
        let (fx, fy, fz) = self.fwd();
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        let g = (std::f64::consts::FRAC_PI_6).tan(); // half-height at d = 1
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        let (nx, ny) = (f64::from(ndc[0]), f64::from(ndc[1]));
        let dir = [
            fx + nx * g * aspect * rx + ny * g * ux,
            fy + nx * g * aspect * ry + ny * g * uy,
            fz + nx * g * aspect * rz + ny * g * uz,
        ];
        let len2 = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        if !len2.is_finite() || len2 < 1e-24 {
            return None;
        }
        let inv = 1.0 / len2.sqrt();
        let dir = [dir[0] * inv, dir[1] * inv, dir[2] * inv];
        let origin = self.eye();
        if !dir.iter().all(|v| v.is_finite()) || !origin.iter().all(|v| v.is_finite()) {
            return None;
        }
        Some((origin, dir))
    }

    /// Dolly toward the cursor, keeping the world point under it fixed.
    /// `ndc` is the cursor in normalized device coords (x right, y up,
    /// both -1..1) and `aspect` the viewport width/height. Returns the new
    /// target, or None on a degenerate ray (caller keeps plain behavior).
    pub fn dolly_cursor(
        &mut self,
        ndc: [f32; 2],
        aspect: f64,
        wheel_steps: f32,
    ) -> Option<[f64; 3]> {
        let (fx, fy, fz) = self.fwd();
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        let g = (std::f64::consts::FRAC_PI_6).tan(); // half-height at d = 1
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        let (nx, ny) = (f64::from(ndc[0]), f64::from(ndc[1]));
        if !nx.is_finite() || !ny.is_finite() {
            return None;
        }
        let dir = [
            fx + nx * g * aspect * rx + ny * g * ux,
            fy + nx * g * aspect * ry + ny * g * uy,
            fz + nx * g * aspect * rz + ny * g * uz,
        ];
        let (tx, ty, tz) = (self.target[0], self.target[1], self.target[2]);
        let (ex, ey, ez) = (self.eye()[0], self.eye()[1], self.eye()[2]);
        // Closest point on the ray to the current target.
        let (wx, wy, wz) = (tx - ex, ty - ey, tz - ez);
        let dd = dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2];
        if !dd.is_finite() || dd <= 1e-12 {
            return None;
        }
        let s = (wx * dir[0] + wy * dir[1] + wz * dir[2]) / dd;
        let (px, py, pz) = (ex + dir[0] * s, ey + dir[1] * s, ez + dir[2] * s);
        let old_dist = self.dist;
        self.zoom(wheel_steps);
        let k = self.dist / old_dist;
        if !k.is_finite() || k <= 0.0 {
            return None;
        }
        self.target = [px + (tx - px) * k, py + (ty - py) * k, pz + (tz - pz) * k];
        if self.target.iter().any(|t| !t.is_finite()) {
            return None;
        }
        Some(self.target)
    }

    /// Center the camera on a bounding sphere.
    #[must_use]
    pub fn frame(center: [f64; 3], radius: f64) -> Camera {
        Camera {
            target: center,
            yaw: 0.9,
            pitch: 0.5,
            dist: (radius * 3.2).clamp(1.0, 50_000.0),
        }
    }

    #[must_use]
    fn eye(&self) -> [f64; 3] {
        let cp = self.pitch.cos();
        [
            self.target[0] + self.dist * cp * self.yaw.cos(),
            self.target[1] + self.dist * cp * self.yaw.sin(),
            self.target[2] + self.dist * self.pitch.sin(),
        ]
    }

    /// Camera position as f32, for the shader's specular term.
    #[must_use]
    pub fn eye_f32(&self) -> [f32; 3] {
        let finite = |v: f64| if v.is_finite() { v as f32 } else { 0.0 };
        std::array::from_fn(|i| finite(self.eye()[i]))
    }

    /// Forward from eye toward target, normalized.
    #[must_use]
    fn fwd(&self) -> (f64, f64, f64) {
        let e = self.eye();
        normalize([self.target[0] - e[0], self.target[1] - e[1], self.target[2] - e[2]])
    }

    /// Right = normalize(cross(forward, +Z)); stable except straight up/down.
    #[must_use]
    fn basis_right(&self) -> (f64, f64, f64) {
        let (fx, fy, _) = self.fwd();
        let (x, y, _) = normalize([fy, -fx, 0.0]);
        if x == 0.0 && y == 0.0 {
            return (1.0, 0.0, 0.0); // looking straight along Z
        }
        (x, y, 0.0)
    }

    /// Up = cross(right, forward).
    #[must_use]
    fn basis_up(&self) -> (f64, f64, f64) {
        let f = {
            let (x, y, z) = self.fwd();
            [x, y, z]
        };
        let (rx, ry, rz) = self.basis_right();
        normalize([ry * f[2] - rz * f[1], rz * f[0] - rx * f[2], rx * f[1] - ry * f[0]])
    }

    /// Column-major model-view-projection (right-handed, camera looks -Z).
    #[must_use]
    pub fn mvp(&self, aspect: f64) -> [[f32; 4]; 4] {
        let e = self.eye();
        let (f0, f1, f2) = self.fwd();
        let (rx, ry, rz) = self.basis_right();
        let (ux, uy, uz) = self.basis_up();
        let view = [
            [rx, ux, -f0, 0.0],
            [ry, uy, -f1, 0.0],
            [rz, uz, -f2, 0.0],
            [
                -(rx * e[0] + ry * e[1] + rz * e[2]),
                -(ux * e[0] + uy * e[1] + uz * e[2]),
                f0 * e[0] + f1 * e[1] + f2 * e[2],
                1.0,
            ],
        ];
        let near = 0.1f64;
        let far = (self.dist * 40.0).max(1000.0);
        let g = (std::f64::consts::FRAC_PI_6).tan().recip(); // 60 deg vertical fov
        let aspect = if aspect.is_finite() && aspect > 1e-6 {
            aspect
        } else {
            1.0
        };
        let proj = [
            [g / aspect, 0.0, 0.0, 0.0],
            [0.0, g, 0.0, 0.0],
            [0.0, 0.0, (far + near) / (near - far), -1.0],
            [0.0, 0.0, 2.0 * far * near / (near - far), 0.0],
        ];
        mat_mul(proj, view)
    }
}

fn normalize(v: [f64; 3]) -> (f64, f64, f64) {
    let l = v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
    if l <= f64::EPSILON {
        return (0.0, 0.0, 0.0);
    }
    let l = l.sqrt();
    (v[0] / l, v[1] / l, v[2] / l)
}

/// Wireframe of a triangle soup: boundary edges (one face) and creases
/// (two faces bending past ~40 degrees) are emitted; facet seams inside a
/// flat or smoothly-bent run stay hidden so shading carries the surface.
/// Endpoints are welded by quantized position (1/4096 mm), so STL's
/// duplicated corners count as shared. Non-manifold edges are drawn rather
/// than silently dropped - a wrong-but-visible line beats a missing one.
fn feature_edges(verts: &[[f32; 3]]) -> Vec<u32> {
    type Pos = (i64, i64, i64);
    type EdgeKey = (Pos, Pos);

    fn q(v: [f32; 3]) -> Pos {
        let s = |x: f32| (f64::from(x) * 4096.0).round() as i64;
        (s(v[0]), s(v[1]), s(v[2]))
    }
    fn edge_key(keys: &[Pos], i: usize, j: usize) -> EdgeKey {
        let (ki, kj) = (keys[i], keys[j]);
        if ki <= kj {
            (ki, kj)
        } else {
            (kj, ki)
        }
    }
    fn unit_normal(verts: &[[f32; 3]], t: usize) -> Option<[f64; 3]> {
        let (a, b, c) = (verts[t * 3], verts[t * 3 + 1], verts[t * 3 + 2]);
        let e1 = [
            f64::from(b[0] - a[0]),
            f64::from(b[1] - a[1]),
            f64::from(b[2] - a[2]),
        ];
        let e2 = [
            f64::from(c[0] - a[0]),
            f64::from(c[1] - a[1]),
            f64::from(c[2] - a[2]),
        ];
        let n = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let len2 = n[0] * n[0] + n[1] * n[1] + n[2] * n[2];
        if len2.is_finite() && len2 > 1e-24 {
            let inv = 1.0 / len2.sqrt();
            Some([n[0] * inv, n[1] * inv, n[2] * inv])
        } else {
            None
        }
    }

    let tri_count = verts.len() / 3;
    if tri_count == 0 {
        return Vec::new();
    }
    let keys: Vec<Pos> = verts.iter().copied().map(q).collect();

    #[derive(Clone, Copy)]
    struct Side {
        first: usize,
        other: usize,
        count: u32,
    }
    let mut shared: HashMap<EdgeKey, Side> = HashMap::new();
    for t in 0..tri_count {
        for (i, j) in [
            (3 * t, 3 * t + 1),
            (3 * t + 1, 3 * t + 2),
            (3 * t + 2, 3 * t),
        ] {
            shared
                .entry(edge_key(&keys, i, j))
                .and_modify(|s| s.other = t)
                .or_insert(Side {
                    first: t,
                    other: t,
                    count: 0,
                })
                .count += 1;
        }
    }

    let cos_crease = 40.0f64.to_radians().cos();
    let mut out: Vec<u32> = Vec::new();
    for t in 0..tri_count {
        for (i, j) in [
            (3 * t, 3 * t + 1),
            (3 * t + 1, 3 * t + 2),
            (3 * t + 2, 3 * t),
        ] {
            let Some(side) = shared.get(&edge_key(&keys, i, j)) else {
                continue;
            };
            if side.first != t {
                continue; // only the owning triangle emits an edge once
            }
            let feature = if side.count == 2 {
                match (unit_normal(verts, t), unit_normal(verts, side.other)) {
                    (Some(x), Some(y)) => {
                        x[0] * y[0] + x[1] * y[1] + x[2] * y[2] < cos_crease
                    }
                    _ => true,
                }
            } else {
                true // boundary (count 1) or non-manifold (count 3+)
            };
            if feature {
                out.extend_from_slice(&[i as u32, j as u32]);
            }
        }
    }
    out
}

/// Column-major matrix product: `out[c][r] = sum_k a[k][r] * b[c][k]`.
/// Non-finite results become 0 rather than poisoning the whole matrix.
fn mat_mul(a: [[f64; 4]; 4], b: [[f64; 4]; 4]) -> [[f32; 4]; 4] {
    std::array::from_fn(|c| {
        std::array::from_fn(|r| {
            let mut s = 0.0f64;
            for k in 0..4 {
                s += a[k][r] * b[c][k];
            }
            if s.is_finite() { s as f32 } else { 0.0 }
        })
    })
}

#[cfg(test)]
mod edge_tests {
    use super::feature_edges;
    use hsmworks_app::geometry::{cube, Point3};

    fn flat_row() -> Vec<[f32; 3]> {
        // A 2x1 rectangle tiled with four coplanar triangles: no interior
        // seam may be drawn, all border edges must be.
        let quad = |x0: f32| {
            [
                [[x0, 0.0, 0.0], [x0 + 1.0, 1.0, 0.0], [x0 + 1.0, 0.0, 0.0]],
                [[x0, 0.0, 0.0], [x0, 1.0, 0.0], [x0 + 1.0, 1.0, 0.0]],
            ]
        };
        quad(0.0).into_iter().chain(quad(1.0)).flatten().collect()
    }

    #[test]
    fn coplanar_seam_is_not_an_edge() {
        // The 2x1 rectangle: 6 border edges, but neither the internal
        // vertical nor either diagonal (all coplanar seams).
        let edges = feature_edges(&flat_row());
        assert_eq!(edges.len() / 2, 6);
    }

    #[test]
    fn closed_cube_shows_its_twelve_box_edges() {
        let mesh = cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0));
        let verts: Vec<[f32; 3]> = mesh
            .facets
            .iter()
            .flat_map(|t| {
                [
                    [t.a.x as f32, t.a.y as f32, t.a.z as f32],
                    [t.b.x as f32, t.b.y as f32, t.b.z as f32],
                    [t.c.x as f32, t.c.y as f32, t.c.z as f32],
                ]
            })
            .collect();
        let edges = feature_edges(&verts);
        assert_eq!(edges.len() / 2, 12, "a cube is 12 crease edges, no seams");
    }

    #[test]
    fn empty_and_degenerate_inputs_stay_finite() {
        assert!(feature_edges(&[]).is_empty());
        let nan = [
            [f32::NAN, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
        ];
        // Degenerate normals are drawn as (suspect) edges, never a panic.
        let e = feature_edges(&nan);
        assert_eq!(e.len(), 6);
    }
}
