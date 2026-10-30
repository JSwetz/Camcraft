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
    Post,
    Stock,
    WorkOffset,
    Tools,
    Face,
    Contour,
    Pocket,
    Slot,
    Trace,
    Drill,
    Finish3D,
    Contour5D,
    Parallel5D,
    Swarf,
    AdvancedSwarf,
    RotaryContour,
    RotaryParallel,
    RotaryOffset,
    MultiAxisRough,
    Flow5D,
    MultiAxisContour,
    Undo,
    Redo,
    Delete,
    Generate,
    FrameAll,
    Toolpaths,
    Model,
    EyeOff,
    Simulate,
    Fixture,
}

impl Icon {
    /// Every slot, ribbon order - the table and the store share this list.
    pub(crate) const ALL: [Icon; 35] = [
        Icon::New,
        Icon::OpenModel,
        Icon::OpenProject,
        Icon::Save,
        Icon::Post,
        Icon::Stock,
        Icon::WorkOffset,
        Icon::Tools,
        Icon::Face,
        Icon::Contour,
        Icon::Pocket,
        Icon::Slot,
        Icon::Trace,
        Icon::Drill,
        Icon::Finish3D,
        Icon::Contour5D,
        Icon::Parallel5D,
        Icon::Swarf,
        Icon::AdvancedSwarf,
        Icon::RotaryContour,
        Icon::RotaryParallel,
        Icon::RotaryOffset,
        Icon::MultiAxisRough,
        Icon::Flow5D,
        Icon::MultiAxisContour,
        Icon::Undo,
        Icon::Redo,
        Icon::Delete,
        Icon::Generate,
        Icon::FrameAll,
        Icon::Toolpaths,
        Icon::Model,
        Icon::EyeOff,
        Icon::Simulate,
        Icon::Fixture,
    ];

    fn png(self) -> &'static [u8] {
        match self {
            Icon::New => include_bytes!("../../../../assets/ui/file-plus.png"),
            Icon::OpenModel => include_bytes!("../../../../assets/ui/folder-open.png"),
            Icon::OpenProject => include_bytes!("../../../../assets/ui/file-import.png"),
            Icon::Save => include_bytes!("../../../../assets/ui/device-floppy.png"),
            // Original drawing (no Tabler "post to G-code" glyph exists):
            // the code chevrons, in the set's stroke weight. Ours to give.
            Icon::Post => include_bytes!("../../../../assets/ui/post.png"),
            Icon::Stock => include_bytes!("../../../../assets/ui/box-model.png"),
            Icon::WorkOffset => include_bytes!("../../../../assets/ui/transform.png"),
            Icon::Tools => include_bytes!("../../../../assets/ui/tools.png"),
            Icon::Face => include_bytes!("../../../../assets/ui/layers-difference.png"),
            Icon::Contour => include_bytes!("../../../../assets/ui/border-outer.png"),
            Icon::Pocket => include_bytes!("../../../../assets/ui/container.png"),
            Icon::Slot => include_bytes!("../../../../assets/ui/minus.png"),
            Icon::Trace => include_bytes!("../../../../assets/ui/route.png"),
            // Original drawing (no Tabler drill-bit exists): a twist
            // drill - shank, shouldered body, one helix wrap, cone
            // point - in the set's stroke weight. Ours to give.
            Icon::Drill => include_bytes!("../../../../assets/ui/drill.png"),
            // Original drawing (no Tabler waterline glyph exists): nested
            // contour rings tightening toward the high point - the map
            // reading of a waterline finish. Ours to give.
            Icon::Finish3D => include_bytes!("../../../../assets/ui/finish3d.png"),
            // Original drawings (no Tabler 5-axis glyph exists): the dome
            // cross-section with the leaning ball-end - flank contour
            // lines for the ring ladder, a straight chord for the
            // raster. Ours to give.
            Icon::Contour5D => include_bytes!("../../../../assets/ui/contour5d.png"),
            Icon::Parallel5D => include_bytes!("../../../../assets/ui/parallel5d.png"),
            // Original drawings (no Tabler swarf glyph exists): the
            // curved blade flank, the tool LYING with it - axis and
            // ruling on one line through the ball - the advanced
            // variant adding the second, shallower rung above. Ours
            // to give.
            Icon::Swarf => include_bytes!("../../../../assets/ui/swarf.png"),
            Icon::AdvancedSwarf => include_bytes!("../../../../assets/ui/advancedswarf.png"),
            // Original drawings (no Tabler rotary glyph exists): the
            // turned bar with the tool riding OUTSIDE it - rings about
            // the barrel for the contour's stations, lines along it for
            // the parallel's raster - the ball on the radial, not the
            // normal. Ours to give.
            Icon::RotaryContour => include_bytes!("../../../../assets/ui/rotarycontour.png"),
            Icon::RotaryParallel => include_bytes!("../../../../assets/ui/rotaryparallel.png"),
            // Original drawing (no Tabler rotary-roughing glyph exists):
            // the turned bar with a STEPPED face - the rough ladder
            // peeling radii, the tool riding outside on the radial.
            // Ours to give.
            Icon::RotaryOffset => include_bytes!("../../../../assets/ui/rotaryoffset.png"),
            // Original drawing (no Tabler tilted-tool glyph exists): the
            // stepped block with a LEANING tool - the ladder, posed.
            // Ours to give.
            Icon::MultiAxisRough => include_bytes!("../../../../assets/ui/multiaxisrough.png"),
            // Original drawing (no Tabler flowline glyph exists): the
            // dome cross-section with the leaning ball-end - the lines
            // CURVING with the surface instead of crossing it, the
            // parallel's chord bent into the blade's own sweep. Ours
            // to give.
            Icon::Flow5D => include_bytes!("../../../../assets/ui/flow5d.png"),
            // Original drawing (no Tabler drive-curve glyph exists): the
            // dome with a CHAIN of straight runs riding its shoulder,
            // dots marking the picked vertices - the path is the picks,
            // no pattern - and the ball leaning off vertical at the
            // last one, the pose law the family shares. Ours to give.
            Icon::MultiAxisContour => include_bytes!("../../../../assets/ui/multiaxiscontour.png"),
            Icon::Undo => include_bytes!("../../../../assets/ui/arrow-back-up.png"),
            Icon::Redo => include_bytes!("../../../../assets/ui/arrow-forward-up.png"),
            Icon::Delete => include_bytes!("../../../../assets/ui/trash.png"),
            Icon::Generate => include_bytes!("../../../../assets/ui/player-play.png"),
            Icon::FrameAll => include_bytes!("../../../../assets/ui/maximize.png"),
            Icon::Toolpaths => include_bytes!("../../../../assets/ui/eye.png"),
            Icon::Model => include_bytes!("../../../../assets/ui/cube.png"),
            Icon::EyeOff => include_bytes!("../../../../assets/ui/eye-off.png"),
            Icon::Simulate => include_bytes!("../../../../assets/ui/sim.png"),
            // Original drawing (no Tabler vise glyph exists): the two
            // jaws, screw shaft and round knob of a machine vise, in the
            // set's stroke weight. Ours to give.
            Icon::Fixture => include_bytes!("../../../../assets/ui/fixture.png"),
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
            Icon::Post => "icon-post",
            Icon::Stock => "icon-stock",
            Icon::WorkOffset => "icon-work-offset",
            Icon::Tools => "icon-tools",
            Icon::Face => "icon-face",
            Icon::Contour => "icon-contour",
            Icon::Pocket => "icon-pocket",
            Icon::Slot => "icon-slot",
            Icon::Trace => "icon-trace",
            Icon::Drill => "icon-drill",
            Icon::Finish3D => "icon-finish3d",
            Icon::Contour5D => "icon-contour5d",
            Icon::Parallel5D => "icon-parallel5d",
            Icon::Swarf => "icon-swarf",
            Icon::AdvancedSwarf => "icon-advanced-swarf",
            Icon::RotaryContour => "icon-rotary-contour",
            Icon::RotaryParallel => "icon-rotary-parallel",
            Icon::RotaryOffset => "icon-rotary-offset",
            Icon::MultiAxisRough => "icon-multiaxis-rough",
            Icon::Flow5D => "icon-flow5d",
            Icon::MultiAxisContour => "icon-multiaxis-contour",
            Icon::Undo => "icon-undo",
            Icon::Redo => "icon-redo",
            Icon::Delete => "icon-delete",
            Icon::Generate => "icon-generate",
            Icon::FrameAll => "icon-frame-all",
            Icon::Toolpaths => "icon-toolpaths",
            Icon::Model => "icon-model",
            Icon::EyeOff => "icon-eye-off",
            Icon::Simulate => "icon-simulate",
            Icon::Fixture => "icon-fixture",
        }
    }
}

/// Upload edge: 2x the token's `glyph_px` (20 display px at the dark
/// defaults - the painted path reads the token, not this number) for
/// clean minification on the button glyphs.
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
