//! camcraft-tools - the tool-catalog seam of the M4.1 crate split
//! (R2, docs/M4.1-CRATE-SPLIT.md). Just the plain types: the R0 audit
//! proved tools.rs imports nothing (not even geometry), and it still
//! holds - the entity wrappers live in the app's schema, the picker
//! and sheet rendering in commands/tool and tool_list.
//!
//! Provenance: end mill / ball nose / bull nose shapes and the
//! coatings/carbide taxonomy are general tool-catalog practice from
//! public machine-tool handbooks and ISO 13399 concepts; nothing from
//! any vendor catalog file. The catalog is user data, not code -
//! this crate only says what a tool record IS.

pub mod cutting;
// #368: a modelled tool body as the stack laws read it.
pub mod holder_profile;
// #93: a drawn revolved (spun) profile for milling tools and holders.
pub mod spun;
// #369: a lathe-held body as the revolved-stock law reads it.
pub mod lathe_holder;
// #396: the insert plate a lathe body's judge leaves out.
pub mod lathe_insert_zone;
pub mod tools;
