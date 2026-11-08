//! The moves of a path in program order (M13.11): the path's own, with
//! each control-compensated span's programmed geometry in place of its
//! cutter-centre moves and the `COMP` records bracketing it. Provenance:
//! our own intermediate (`docs/CNC-INTERMEDIATE.md`); the engage /
//! cancel placement follows RS-274 / ISO 6983 G41 / G42 / G40 as
//! `cl::comp` states it.

use super::cnc::Record;
use crate::cl::{Move, Toolpath};

/// `(cycle anchor index, the move, the record that precedes it)` for
/// every motion record the dump writes. A span that does not chain
/// onto its moves is not trusted: its centre moves post as a centre
/// path - a correct program - and a `WARN` pushed onto `out` says the
/// compensation stood down, so the gate holds the program.
pub(super) fn program_order<'a>(t: &'a Toolpath, out: &mut Vec<Record>) -> Vec<(usize, &'a Move, Option<Record>)> {
    let mut spans: Vec<&crate::cl::CompSpan> = t.comps.iter().filter(|c| c.is_sane(&t.moves)).collect();
    spans.sort_by_key(|c| c.first);
    for c in &t.comps {
        if !c.is_sane(&t.moves) {
            out.push(Record::Warn {
                text: "control compensation: a compensated span did not chain onto its moves - refused".into(),
            });
        }
    }
    let mut si = 0usize;
    let mut program: Vec<(usize, &Move, Option<Record>)> = Vec::with_capacity(t.moves.len());
    let mut mi = 0usize;
    while mi < t.moves.len() {
        if let Some(c) = spans.get(si).filter(|c| c.first == mi) {
            let n = c.programmed.len();
            for (k, pm) in c.programmed.iter().enumerate() {
                let pre = if k == 0 {
                    Some(Record::Comp { side: Some(c.side), radius_mm: c.radius, wear: c.wear })
                } else if k + 1 == n {
                    Some(Record::Comp { side: None, radius_mm: 0.0, wear: false })
                } else {
                    None
                };
                // Cycles anchor on the engage index; every programmed
                // move after it shares the cancel's slot.
                program.push((if k == 0 { c.first } else { c.last }, pm, pre));
            }
            mi = c.last + 1;
            si += 1;
            continue;
        }
        program.push((mi, &t.moves[mi], None));
        mi += 1;
    }
    program
}
