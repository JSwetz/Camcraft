//! The UI icon set, folded into the egui stack at M1.4.2 M-g: Tabler
//! outline glyphs (MIT, see assets/ui/LICENSES.md) bundled as PNGs,
//! recolored to white line art and loaded as egui textures.
//!
//! Provenance: the loader is our own, ported from the retired imgui
//! icons.rs; the glyphs are Tabler Icons, unmodified except channel order
//! (AGENTS.md clean-room: never any HSMWorks asset).
//!
//! The imgui loader hand-built wgpu textures and registered them as
//! external ids; egui owns that machinery, so `TextureHandle`s come from
//! `ctx.load_texture` (the renderer's texture manager uploads and
//! re-uploads as needed). The law carried over unchanged: building the
//! set is a lazy GPU job (a headless or pre-GPU frame holds `None`), a
//! glyph that fails to decode is dropped to `None` with a diagnostic -
//! never the whole ribbon - and every consumer must degrade to its
//! painted fallback, never to nothing.

use egui::{ColorImage, Context, TextureHandle, TextureOptions};

/// One bundled glyph. The ribbon is the first consumer; the browser tree
/// pass added slots here, not elsewhere - the PNG table lives once.
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
    Model,
    EyeOff,
}

impl Icon {
    /// Every slot, ribbon order - the table and the store share this list.
    pub(crate) const ALL: [Icon; 20] = [
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
        Icon::Model,
        Icon::EyeOff,
    ];

    fn png(self) -> &'static [u8] {
        match self {
            Icon::New => include_bytes!("../../../../assets/ui/file-plus.png"),
            Icon::OpenModel => include_bytes!("../../../../assets/ui/folder-open.png"),
            Icon::OpenProject => include_bytes!("../../../../assets/ui/file-import.png"),
            Icon::Save => include_bytes!("../../../../assets/ui/device-floppy.png"),
            Icon::Stock => include_bytes!("../../../../assets/ui/box-model.png"),
            Icon::WorkOffset => include_bytes!("../../../../assets/ui/transform.png"),
            Icon::Tools => include_bytes!("../../../../assets/ui/tools.png"),
            Icon::Face => include_bytes!("../../../../assets/ui/layers-difference.png"),
            Icon::Contour => include_bytes!("../../../../assets/ui/border-outer.png"),
            Icon::Pocket => include_bytes!("../../../../assets/ui/container.png"),
            Icon::Slot => include_bytes!("../../../../assets/ui/minus.png"),
            Icon::Trace => include_bytes!("../../../../assets/ui/route.png"),
            Icon::Undo => include_bytes!("../../../../assets/ui/arrow-back-up.png"),
            Icon::Redo => include_bytes!("../../../../assets/ui/arrow-forward-up.png"),
            Icon::Delete => include_bytes!("../../../../assets/ui/trash.png"),
            Icon::Generate => include_bytes!("../../../../assets/ui/player-play.png"),
            Icon::FrameAll => include_bytes!("../../../../assets/ui/maximize.png"),
            Icon::Toolpaths => include_bytes!("../../../../assets/ui/eye.png"),
            Icon::Model => include_bytes!("../../../../assets/ui/cube.png"),
            Icon::EyeOff => include_bytes!("../../../../assets/ui/eye-off.png"),
        }
    }

    fn name(self) -> &'static str {
        // The egui texture registry key; stable per slot, never reused
        // for other pixels (a rename here would alias old uploads).
        match self {
            Icon::New => "icon-new",
            Icon::OpenModel => "icon-open-model",
            Icon::OpenProject => "icon-open-project",
            Icon::Save => "icon-save",
            Icon::Stock => "icon-stock",
            Icon::WorkOffset => "icon-work-offset",
            Icon::Tools => "icon-tools",
            Icon::Face => "icon-face",
            Icon::Contour => "icon-contour",
            Icon::Pocket => "icon-pocket",
            Icon::Slot => "icon-slot",
            Icon::Trace => "icon-trace",
            Icon::Undo => "icon-undo",
            Icon::Redo => "icon-redo",
            Icon::Delete => "icon-delete",
            Icon::Generate => "icon-generate",
            Icon::FrameAll => "icon-frame-all",
            Icon::Toolpaths => "icon-toolpaths",
            Icon::Model => "icon-model",
            Icon::EyeOff => "icon-eye-off",
        }
    }
}

/// Display size, in logical px; sources are uploaded at 2x for clean
/// minification on the 20px button glyphs.
pub(crate) const ICON_PX: f32 = 20.0;
const UPLOAD_PX: u32 = 40;

/// Loaded icon set: one handle per slot, parallel to [`Icon::ALL`], with
/// `None` where a glyph failed to decode. A headless frame never builds
/// one at all, so every consumer takes `Option<&IconSet>`.
pub(crate) struct IconSet {
    handles: Vec<Option<TextureHandle>>,
}

impl IconSet {
    /// Decode, recolor and register every bundled glyph. Called once,
    /// lazily, from the first GPU-backed frame.
    pub(crate) fn load(ctx: &Context) -> Self {
        let handles = Icon::ALL
            .iter()
            .map(|icon| match Self::upload(ctx, *icon) {
                Ok(handle) => Some(handle),
                Err(err) => {
                    eprintln!("icon {icon:?} unavailable: {err}");
                    None
                }
            })
            .collect();
        IconSet { handles }
    }

    fn upload(ctx: &Context, icon: Icon) -> Result<TextureHandle, String> {
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
        let mut rgba = small.to_vec();
        for px in rgba.as_chunks_mut::<4>().0 {
            px[0] = 255;
            px[1] = 255;
            px[2] = 255;
        }
        let color_image = ColorImage::from_rgba_unmultiplied([UPLOAD_PX as usize; 2], &rgba);
        Ok(ctx.load_texture(
            icon.name(),
            color_image,
            TextureOptions::default(),
        ))
    }

    /// Texture for a slot (`None` after a failed decode; every consumer
    /// must degrade to its painted fallback, never to nothing).
    pub(crate) fn texture(&self, icon: Icon) -> Option<&TextureHandle> {
        let i = Icon::ALL.iter().position(|s| *s == icon)?;
        self.handles.get(i)?.as_ref()
    }
}
