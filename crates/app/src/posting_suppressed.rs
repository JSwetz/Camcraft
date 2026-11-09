//! The suppression clause of the verification gate (#110 review B1): a
//! posted operation whose verdicts read the program ahead was generated
//! against every earlier same-setup cut, suppressed ones included. Left
//! out of the NC, that cut is never made - the later op's rapids and
//! full-engagement passes would land in stock nothing judged - so the
//! post refuses by name instead of assuming the part is already cut.
//!
//! Provenance: our own design notes (the rest-machining dependency of
//! `rest.rs` read back at post time); no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;

/// One line per (posted op, suppressed cut ahead of it) pair, in
/// program order. Empty when nothing suppressed cut ahead of an op that
/// reads the program ahead.
#[must_use]
pub(super) fn suppressed_problems(world: &World) -> Vec<String> {
    let mut out = Vec::new();
    for (name, e) in crate::setup::operation_order(world) {
        let Some(op) = model::operation_of(world, &e) else {
            continue;
        };
        // A suppressed op posts nothing, and an op whose verdicts never
        // read the program ahead (a posed op) assumed nothing of it.
        if op.suppressed || !crate::operation::reads_prior(&op.params) {
            continue;
        }
        for gone in crate::rest::suppressed_cuts_ahead(world, &name, op.setup.as_deref()) {
            out.push(format!(
                "operation {name} was generated against the cut of suppressed {gone}; \
                 unsuppress {gone} or suppress {name} too"
            ));
        }
    }
    out
}
