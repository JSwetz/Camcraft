//! UI icon set: Tabler outline glyphs (MIT, see assets/ui/LICENSES.md)
//! bundled as 240px PNGs, recolored to white line art and uploaded as
//! external ImGui textures at startup.
//!
//! Provenance: our own loader; the glyphs are Tabler Icons, unmodified
//! except channel order (AGENTS.md clean-room: never any HSMWorks asset).
//!
//! The renderer lives in app.rs, so building the set is an app-init job:
//! `State::icons` stays `None` headless (the legacy atlas path has no
//! external textures), and every consumer must handle `None` by drawing
//! text only.

use dear_imgui_rs::TextureId;
use dear_imgui_wgpu::wgpu;

/// One bundled glyph. The ribbon is the current consumer; the browser tree
/// pass adds slots here, not elsewhere - the PNG table lives once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Icon {
    New,
    OpenModel,
    OpenProject,
    Save,
    Stock,
    WorkOffset,
    Tools,
    Face,
    Contour,
    Pocket,
    Slot,
    Trace,
    Undo,
    Redo,
    Delete,
    Generate,
    FrameAll,
    Toolpaths,
}

impl Icon {
    /// Every slot, ribbon order - the table and the store share this list.
    pub(crate) const ALL: [Icon; 18] = [
        Icon::New,
        Icon::OpenModel,
        Icon::OpenProject,
        Icon::Save,
        Icon::Stock,
        Icon::WorkOffset,
        Icon::Tools,
        Icon::Face,
        Icon::Contour,
        Icon::Pocket,
        Icon::Slot,
        Icon::Trace,
        Icon::Undo,
        Icon::Redo,
        Icon::Delete,
        Icon::Generate,
        Icon::FrameAll,
        Icon::Toolpaths,
    ];

    fn png(self) -> &'static [u8] {
        match self {
            Icon::New => include_bytes!("../../../assets/ui/file-plus.png"),
            Icon::OpenModel => include_bytes!("../../../assets/ui/folder-open.png"),
            Icon::OpenProject => include_bytes!("../../../assets/ui/file-import.png"),
            Icon::Save => include_bytes!("../../../assets/ui/device-floppy.png"),
            Icon::Stock => include_bytes!("../../../assets/ui/box-model.png"),
            Icon::WorkOffset => include_bytes!("../../../assets/ui/transform.png"),
            Icon::Tools => include_bytes!("../../../assets/ui/tools.png"),
            Icon::Face => include_bytes!("../../../assets/ui/layers-difference.png"),
            Icon::Contour => include_bytes!("../../../assets/ui/border-outer.png"),
            Icon::Pocket => include_bytes!("../../../assets/ui/container.png"),
            Icon::Slot => include_bytes!("../../../assets/ui/minus.png"),
            Icon::Trace => include_bytes!("../../../assets/ui/route.png"),
            Icon::Undo => include_bytes!("../../../assets/ui/arrow-back-up.png"),
            Icon::Redo => include_bytes!("../../../assets/ui/arrow-forward-up.png"),
            Icon::Delete => include_bytes!("../../../assets/ui/trash.png"),
            Icon::Generate => include_bytes!("../../../assets/ui/player-play.png"),
            Icon::FrameAll => include_bytes!("../../../assets/ui/maximize.png"),
            Icon::Toolpaths => include_bytes!("../../../assets/ui/eye.png"),
        }
    }
}

/// Display size, in logical px; sources are uploaded at 2x for clean
/// minification on the 20px button glyphs.
pub(crate) const ICON_PX: f32 = 20.0;
const UPLOAD_PX: u32 = 40;

/// Loaded icon set: wgpu textures kept alive (registration must not
/// outlive them) plus the ImGui ids, parallel to [`Icon::ALL`]. The type
/// is public (it is a field of the public `State`) but everything useful
/// on it is crate-internal, and headless builds hold `None`.
pub struct Icons {
    _textures: Vec<wgpu::Texture>,
    ids: Vec<Option<TextureId>>,
}

impl Icons {
    /// Decode, recolor, upload every bundled glyph. A glyph that fails to
    /// decode is dropped to `None` (drawn as text only) with a diagnostic -
    /// never a reason to lose the whole ribbon.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut dear_imgui_wgpu::WgpuRenderer,
    ) -> Self {
        let mut textures = Vec::with_capacity(Icon::ALL.len());
        let mut ids = Vec::with_capacity(Icon::ALL.len());
        for icon in Icon::ALL {
            match Self::upload(icon, device, queue, renderer) {
                Ok((tex, id)) => {
                    textures.push(tex);
                    ids.push(Some(id));
                }
                Err(err) => {
                    eprintln!("icon {:?} unavailable: {err}", icon);
                    ids.push(None);
                }
            }
        }
        Icons {
            _textures: textures,
            ids,
        }
    }

    fn upload(
        icon: Icon,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        renderer: &mut dear_imgui_wgpu::WgpuRenderer,
    ) -> Result<(wgpu::Texture, TextureId), String> {
        let img = image::load_from_memory(icon.png())
            .map_err(|e| e.to_string())?
            .into_rgba8();
        let small = image::imageops::resize(
            &img,
            UPLOAD_PX,
            UPLOAD_PX,
            image::imageops::FilterType::Lanczos3,
        );
        // Tabler strokes are black with alpha coverage; the chrome is dark,
        // so repaint every pixel white and let the alpha do the drawing.
        let mut data = small.into_raw();
        for px in data.as_chunks_mut::<4>().0 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        }
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("ui-icon"),
            size: wgpu::Extent3d {
                width: UPLOAD_PX,
                height: UPLOAD_PX,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(UPLOAD_PX * 4),
                rows_per_image: Some(UPLOAD_PX),
            },
            wgpu::Extent3d {
                width: UPLOAD_PX,
                height: UPLOAD_PX,
                depth_or_array_layers: 1,
            },
        );
        let external = renderer
            .register_external_texture(&texture.create_view(&wgpu::TextureViewDescriptor::default()))
            .map_err(|e| e.to_string())?;
        Ok((texture, external.texture_id()))
    }

    pub(crate) fn texture(&self, icon: Icon) -> Option<TextureId> {
        let i = Icon::ALL.iter().position(|s| *s == icon)?;
        self.ids.get(i).copied().flatten()
    }
}
