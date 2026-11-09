//! #389: a setup's own post options as files hold them. The key is absent
//! when the setup follows the document's options (every earlier file, and
//! every setup never given its own), so the absent key is the whole
//! migration and an untouched document saves byte for byte as before -
//! additive, no version step, like the document's #110 / #111 switches.
//! Owned options are spelled in full: absent already means "follows".
//!
//! Provenance: our own file grammar (the `PostOptionsDto` words restated
//! for one setup); no vendor material consulted.

use serde::{Deserialize, Serialize};

use crate::schema::{PostOptions, ThreadingOutput};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SetupPostOptionsDto {
    /// `"g32"` (the passes) or `"g76"` (the cycle).
    pub threading: String,
    pub skip_stops: bool,
    pub per_op_files: bool,
}

impl SetupPostOptionsDto {
    #[must_use]
    pub fn from_options(o: &PostOptions) -> Self {
        Self {
            threading: match o.threading {
                ThreadingOutput::G32Passes => "g32".to_owned(),
                ThreadingOutput::G76Cycle => "g76".to_owned(),
            },
            skip_stops: o.skip_stops,
            per_op_files: o.per_op,
        }
    }

    /// The live value; an unspellable thread output is a corrupt file,
    /// refused by the setup's name.
    pub fn to_options(&self, setup: &str) -> Result<PostOptions, String> {
        let threading = match self.threading.as_str() {
            "g32" => ThreadingOutput::G32Passes,
            "g76" => ThreadingOutput::G76Cycle,
            other => return Err(format!("setup '{setup}' names an unknown threading output {other:?}")),
        };
        Ok(PostOptions { threading, skip_stops: self.skip_stops, per_op: self.per_op_files })
    }
}
