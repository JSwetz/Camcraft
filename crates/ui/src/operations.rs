//! The 2D operation parameter law: the type-change rule (`retype`), the
//! quick-add seed (`quick_params`), the wording of a height row
//! (`offset_label`, `height_echo`, `unresolved_note`) and the display-side
//! warning fold (`fold_warnings`). The panels that consume these live in
//! `egui_app::pages` and `egui_app::panes`; the imgui originals retired at
//! M1.4.2 M-g, so this module is where each string and default exists
//! exactly once, for whichever surface paints it.
//!
//! Provenance: our own law, specced from public CAM context-property
//! pages; parameter names follow the published machining vocabulary
//! (stepover, stock to leave, climb/conventional, lead in/out, tabs).
//!
//! Like every part of the UI layer this never touches the world ad hoc:
//! surfaces edit the `State` drafts and queue `CommandKind`s, and the
//! `sync_*_ui` methods refetch the truth from the components. Status and
//! warnings are computed on read, so a page cannot display a stale claim
//! about a stale path.

use flecs_ecs::prelude::World;
use camcraft_app::operation::{self, Height, HeightReference, OpParams};
use camcraft_app::units;
use camcraft_app::verify::Warning;

/// The Strategy combo's labels, in index order - one source for the
/// labels, the combo, and any test checking what the pane offers. Auto
/// sits beside the explicit choices; picking it stores the choice, and
/// [`camcraft_app::machining2d::PocketStrategy::resolve`] answers which
/// path it cuts at generate.
pub fn strategy_names() -> &'static [&'static str] {
    &["Zigzag", "Waterline", "Auto"]
}
/// The 3-D pocket's Strategy combo, in index order. Adaptive joins the
/// family (M2.1c); Auto stays last, the same shape as
/// [`strategy_names`], because it is the same kind of choice - stored,
/// resolved at generate.
pub fn strategy3d_names() -> &'static [&'static str] {
    &["Zigzag", "Waterline", "Adaptive", "Auto"]
}
/// The Side combo's labels, in index order (M1.5.4 issue 2). Auto sits
/// beside the explicit choices like the pocket's Strategy: picking it
/// stores the choice, and [`camcraft_app::operation::contour_auto_side`]
/// (the very walk generation runs) answers which side a contour takes
/// at generate.
pub fn side_names() -> &'static [&'static str] {
    &["Auto", "Inside", "Outside"]
}

/// The Cycle combo's labels, in `CycleChoice` index order (M1.6). Auto
/// again sits beside the explicit choices - and it is the tool that
/// answers for it ([`camcraft_app::drilling::auto_kind`]: a tap cycle
/// for a tap, a boring cycle for a bore, depth-vs-diameter otherwise),
/// so the note next to the combo reads from the op's tool, not a guess.
pub fn cycle_names() -> &'static [&'static str] {
    &["Auto", "Spot", "Drill", "Peck", "Tap", "Bore", "Back bore"]
}

/// Combo index for a cycle choice - the inverse of what picking names.
pub(crate) fn cycle_index(c: &camcraft_app::drilling::CycleChoice) -> usize {
    use camcraft_app::drilling::CycleChoice as C;
    match c {
        C::Auto => 0,
        C::Spot => 1,
        C::Drill => 2,
        C::Peck => 3,
        C::Tap => 4,
        C::Bore => 5,
        C::BackBore => 6,
    }
}

/// The row-2 caption: under `Fixed` the number *is* the world z, so do
/// not call it an offset from something (and under the other references
/// never pretend the box holds an absolute plane). Pure so a test can pin
/// which reference reads as which caption.
pub(crate) fn offset_label(reference: HeightReference) -> &'static str {
    if matches!(reference, HeightReference::Fixed) {
        "Value"
    } else {
        "Offset"
    }
}

/// What a height row echoes after its offset box: the plane it resolves
/// to, or - never a stale or guessed number - what is missing. Pure so
/// the tests can pin the exact wording; the draw code only labels it.
pub(crate) fn height_echo(world: &World, setup: Option<&str>, h: &Height) -> String {
    match h.z_for(world, setup) {
        Some(z) => format!("(= Z {})", units::len(z, units::of(world), 3)),
        None => format!(
            "(unresolved: {})",
            operation::missing_source(world, setup, h.reference).unwrap_or("source missing")
        ),
    }
}

/// The pane-wide counterpart of the row echo: while any height fails to
/// resolve, one line tells the whole panel that the viewport dropped a
/// plane and Generate will refuse, naming the field and the reason (the
/// app's `resolve_params` error). `None` when everything resolves, so a
/// healthy panel shows no alarm.
pub(crate) fn unresolved_note(
    world: &World,
    setup: Option<&str>,
    params: &OpParams,
) -> Option<String> {
    operation::resolve_params(world, setup, params)
        .err()
        .map(|why| format!("{why}; set the source up or switch the reference"))
}

/// Swap the draft's parameter variant, carrying over every shared number
/// so re-typing an operation does not throw away feeds and heights.
///
/// Heights are references, not baked world numbers: a fresh op's Top is
/// the part's own top surface and its Bottom the first work offset (G54),
/// the planes the user actually points at - the complaint this replaces
/// was an "arbitrary bottom" computed from the stock. Carried-over
/// heights keep the reference they were saved with. Nothing resolves to
/// a `top - 5.0` anymore; Face cuts one skim at the stock top (`cut_z`
/// is a single plane, so a deeper reference would cut through the whole
/// stock in the default op - the same safe z retype produced before).
///
/// One law, both callers: the page's type combo and the ribbon's Create
/// cells (via `crate::actions::add_operation`) both come through here.
pub(crate) fn retype(cur: &OpParams, kind: usize) -> OpParams {
    camcraft_app::operation::retype_params(cur, kind)
}

/// Parameter set a ribbon quick-add starts an operation of `kind` with:
/// the draft defaults, retyped - the same heights the creation form
/// would have prefilled. No world argument: the seeds are references,
/// and generation resolves them against whatever the document holds.
pub(crate) fn quick_params(kind: usize) -> OpParams {
    retype(&crate::state::OpUi::default().params, kind)
}

/// One line per warning kind, not one per colliding move. verify records
/// a ShankHit for every feed move the holder grazes stock on, so a deep
/// finishing pass used to flood the pane with a wall of near-identical
/// lines. The `Vec<Warning>` itself is the persisted cache format
/// (project `OperationDto.cache`) - the fold lives here, in the display
/// layer, and the full list still round-trips through the project file.
/// Groups keep first-appearance order; `ShankHit`'s two flavors (stock
/// vs part/island) group apart because they read differently and mean
/// different fixes.
pub fn fold_warnings(warnings: &[Warning]) -> Vec<String> {
    fn key(w: &Warning) -> (u8, bool) {
        match w {
            Warning::Crossing { .. } => (0, false),
            Warning::Gouge { .. } => (1, false),
            Warning::ShankHit { part, .. } => (2, *part),
            Warning::RapidInStock { .. } => (3, false),
            Warning::NonFinite { .. } => (4, false),
            Warning::Envelope { .. } => (5, false),
            // Fixture hits group by the part that hit (M3.2 stage B): a
            // shank against a vise reads differently from a rapid into
            // the table, and the fixes differ too.
            Warning::FixtureHit { fixture_part, .. } => (6 + *fixture_part as u8, false),
        }
    }
    fn last_move(w: &Warning) -> usize {
        match w {
            Warning::Crossing { moves } => moves[1],
            Warning::Gouge { at }
            | Warning::ShankHit { at, .. }
            | Warning::RapidInStock { at }
            | Warning::FixtureHit { at, .. }
            | Warning::NonFinite { at }
            | Warning::Envelope { at, .. } => *at,
        }
    }
    let mut lines: Vec<String> = Vec::new();
    let mut groups: Vec<((u8, bool), usize, usize)> = Vec::new();
    for w in warnings {
        match groups.iter_mut().find(|g| g.0 == key(w)) {
            Some(g) => {
                g.1 += 1;
                g.2 = last_move(w);
            }
            None => {
                groups.push((key(w), 1, last_move(w)));
                lines.push(format!("warning: {w}"));
            }
        }
    }
    for (line, (_, count, last)) in lines.iter_mut().zip(&groups) {
        let extra = count - 1;
        if extra > 0 {
            let noun = if extra == 1 { "move" } else { "moves" };
            line.push_str(&format!(" - repeated on {extra} more {noun} (to {last})"));
        }
    }
    lines
}

#[cfg(test)]
mod height_row_tests {
    //! M1.5.3 step 3: the wording of a height row and its pane-wide note.
    //! An unresolved reference must say which source is missing and where
    //! to fix it - never show a stale or guessed z.

    use super::*;
    use camcraft_app::machining2d::{Lead, Side};
    use camcraft_app::world::new_world;

    #[test]
    fn a_resolved_height_echoes_the_plane_it_lands_on() {
        let world = new_world();
        assert_eq!(height_echo(&world, None, &Height::fixed(-7.0)), "(= Z -7.000)");
        assert_eq!(height_echo(&world, None, &Height::fixed(25.0)), "(= Z 25.000)");
    }

    #[test]
    fn only_the_fixed_reference_calls_its_number_a_value() {
        // M1.5.4 row 2: every other reference is From + Offset; Fixed
        // holds the world z itself and must not be captioned "Offset".
        assert_eq!(offset_label(HeightReference::Fixed), "Value");
        for r in [
            HeightReference::TopOfStock,
            HeightReference::BottomOfStock,
            HeightReference::TopOfModel,
            HeightReference::BottomOfModel,
            HeightReference::WorkOffset(0),
        ] {
            assert_eq!(offset_label(r), "Offset", "{r:?} is a source");
        }
    }

    #[test]
    fn a_missing_source_names_itself_and_where_to_fix_it() {
        let world = new_world();
        assert_eq!(
            height_echo(&world, None, &Height::of(HeightReference::TopOfStock, 25.0)),
            "(unresolved: no stock yet - set it up in Setup > Stock)"
        );
        assert_eq!(
            height_echo(&world, None, &Height::of(HeightReference::BottomOfModel, 0.0)),
            "(unresolved: no model loaded)"
        );
        assert_eq!(
            height_echo(&world, None, &Height::of(HeightReference::WorkOffset(0), 0.0)),
            "(unresolved: that work offset has no origin yet - place it in Setup)"
        );
    }

    #[test]
    fn the_pane_note_appears_only_while_something_is_unresolved() {
        let world = new_world();
        let ok = OpParams::Face {
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(19.5),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        assert_eq!(unresolved_note(&world, None, &ok), None, "all fixed - no alarm");
        let broken = OpParams::Face {
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::of(HeightReference::BottomOfStock, 0.0),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        let note = unresolved_note(&world, None, &broken).expect("the stock is gone");
        assert!(note.contains("Cut Z"), "{note}");
        assert!(note.contains("no stock yet"), "{note}");
        assert!(note.ends_with("switch the reference"), "{note}");
    }

    #[test]
    fn fresh_depth_operations_default_to_the_part_and_g54() {
        // The contract behind issue 2: a new operation's Bottom rides the
        // selected work offset and its Top is the part itself - the
        // number the newcomer sees is where the material actually is,
        // never an arbitrary 5 under the stock top.
        for kind in [1usize, 2, 3, 4] {
            let p = quick_params(kind);
            assert_eq!(
                operation::top_of(&p).map(|h| h.reference),
                Some(HeightReference::TopOfModel),
                "kind {kind} tops out at the model",
            );
            assert_eq!(
                operation::bottom_of(&p).map(|h| (h.reference, h.offset)),
                Some((HeightReference::WorkOffset(0), 0.0)),
                "kind {kind} bottoms out at G54",
            );
            assert_eq!(
                p.clearance().reference,
                HeightReference::TopOfStock,
                "rapids stage above the stock",
            );
        }
        let OpParams::Face { cut_z, top_z, .. } = quick_params(0) else {
            panic!("kind 0 is a Face");
        };
        assert_eq!(
            cut_z.reference,
            HeightReference::TopOfStock,
            "a fresh face skims the stock top - a Bottom default would cut through everything",
        );
        assert!(top_z.is_none(), "staging is opt-in, never a surprise default");
    }

    #[test]
    fn a_fresh_drill_starts_empty_auto_and_floorless() {
        // Kind 5's own contract (M1.6): the shared law covers it - R
        // plane at the model top, rapids above the stock - but its
        // bottom is deliberately None: a drilled hole's floor is the
        // hole's own, recorded by recognition, not a work-offset
        // plane. Creation with no holes is legal (the generator
        // refuses to run one, by name), and the cycle rides Auto.
        let OpParams::Drill { top_z, bottom_z, clearance_z, cycle, holes, peck, dwell, retract_z, .. } =
            quick_params(5)
        else {
            panic!("kind 5 is a Drill");
        };
        assert_eq!(top_z.reference, HeightReference::TopOfModel);
        assert!(bottom_z.is_none(), "hole floors ride the holes, not G54");
        assert_eq!(clearance_z.reference, HeightReference::TopOfStock);
        assert!(retract_z.is_none(), "None hops to clearance, like the pocket family");
        assert_eq!(cycle, camcraft_app::drilling::CycleChoice::Auto);
        assert!(holes.is_empty());
        assert_eq!((peck, dwell), (0.0, 0.0), "0 peck lets the Auto law size it; no dwell");
    }

    /// A fixed-height contour with the numbers the retype law carries.
    fn cont_seed() -> OpParams {
        OpParams::Contour {
            side: Side::Auto,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(20.0),
            retract_z: Height::fixed(8.0),
            top_z: Height::fixed(-1.0),
            bottom_z: Height::fixed(-9.0),
            stepdown: 2.0,
            feed: 700.0,
            plunge_feed: 90.0,
            climb: false,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 5.0,
            tab_height: 1.0,
        }
    }

    #[test]
    fn retype_to_and_from_drill_carries_the_shared_numbers() {
        // The retype law's drill legs: feeds and the R plane survive
        // contour -> drill -> pocket; the drill's hole list can only
        // arrive through a command, never through a type change; and a
        // drill's climb-less nature retypes to the milling default.
        let OpParams::Drill { top_z, feed, plunge_feed, holes, cycle, retract_z, .. } =
            retype(&cont_seed(), 5)
        else {
            panic!("kind 5 is a Drill");
        };
        assert_eq!(top_z, Height::fixed(-1.0), "the contour's top becomes the R plane");
        assert_eq!((feed, plunge_feed), (700.0, 90.0));
        assert!(holes.is_empty(), "a type change never invents a hole list");
        assert_eq!(cycle, camcraft_app::drilling::CycleChoice::Auto);
        assert!(retract_z.is_none(), "like the pocket family, a fresh hop plane is None");
        let OpParams::Pocket { top_z, feed, climb, stepdown, .. } =
            retype(&retype(&cont_seed(), 5), 2)
        else {
            panic!("kind 2 is a Pocket");
        };
        assert_eq!(feed, 700.0);
        assert!(climb, "a drill has no climb - the milling default wins");
        assert_eq!(top_z, Height::fixed(-1.0), "and its R plane rides on as the pocket top");
        assert_eq!(stepdown, 5.0, "a drill has no stepdown - the fresh default");
    }
}

#[cfg(test)]
mod warning_fold_tests {
    //! M1.5.4 step 1: the display-side fold. The wall of identical
    //! "shank/holder at move N" lines was the hand-test complaint; the
    //! cache still stores every warning (file-format contract), the pane
    //! must not print every one.

    use super::fold_warnings;
    use camcraft_app::verify::Warning;

    fn shank_run(from: usize, to: usize, part: bool) -> Vec<Warning> {
        (from..=to)
            .map(|at| Warning::ShankHit { at, part })
            .collect()
    }

    #[test]
    fn empty_and_singles_need_no_count() {
        assert!(fold_warnings(&[]).is_empty());
        assert_eq!(
            fold_warnings(&[Warning::Gouge { at: 7 }]),
            ["warning: move 7 cuts past the finish surface"]
        );
    }

    #[test]
    fn a_run_folds_to_its_first_line_plus_count() {
        let folded = fold_warnings(&shank_run(1131, 1146, false));
        assert_eq!(folded.len(), 1, "sixteen hits read as one line");
        assert_eq!(
            folded[0],
            "warning: shank/holder at move 1131 collides with the uncut stock \
             - repeated on 15 more moves (to 1146)"
        );
    }

    #[test]
    fn one_extra_repeat_reads_singular() {
        let folded = fold_warnings(&shank_run(4, 5, false));
        assert_eq!(
            folded[0],
            "warning: shank/holder at move 4 collides with the uncut stock \
             - repeated on 1 more move (to 5)"
        );
    }

    #[test]
    fn flavors_group_apart_and_order_is_first_appearance() {
        let mut w = shank_run(10, 12, false);
        w.push(Warning::ShankHit { at: 500, part: true });
        w.push(Warning::RapidInStock { at: 3 });
        w.push(Warning::ShankHit { at: 540, part: true });
        let folded = fold_warnings(&w);
        assert_eq!(folded.len(), 3);
        assert!(
            folded[0].starts_with("warning: shank/holder at move 10 collides with the uncut stock"),
            "{folded:?}"
        );
        // The island flavor leads with its own first hit, its run's end rides
        // the suffix, and the interleave does not reorder the kinds.
        assert_eq!(
            folded[1],
            "warning: shank/holder at move 500 collides with the part or an island \
             - repeated on 1 more move (to 540)"
        );
        assert_eq!(
            folded[2],
            "warning: rapid move 3 travels below the clearance plane inside stock"
        );
    }
}
