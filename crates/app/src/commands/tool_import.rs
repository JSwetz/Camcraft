//! #300: import a tool library file as ONE undo unit. The library was
//! already checked whole by `tool_library::parse_tool_library`; here each
//! tool meets the document. The duplicate law: a name the document
//! already uses is never overwritten - an identical tool of that name is
//! skipped (it is already here), a different one comes in renamed with
//! the first free ` (2)`, ` (3)`... suffix, so an operation holding the
//! old tool keeps exactly the tool it had.
//!
//! Provenance: our own command model (`AddTool`'s snapshot-free add and
//! remove-by-name inverse, batched); no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::commands::tool::reserved;
use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::ToolDef;
use crate::tool_library::LibraryTool;
use crate::tools::Tool;

/// What an import did with each library tool, for the status line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportReport {
    /// Tools added under their own name.
    pub added: Vec<String>,
    /// `(library name, name it was added as)` for a name already taken.
    pub renamed: Vec<(String, String)>,
    /// Library names skipped: the document holds that exact tool already.
    pub skipped: Vec<String>,
}

impl ImportReport {
    /// One line: "imported 3 tools; 1 renamed (A -> A (2)); 2 already in the document (B, C)".
    #[must_use]
    pub fn summary(&self) -> String {
        let n = self.added.len().saturating_add(self.renamed.len());
        let mut s = format!("imported {n} tool{}", if n == 1 { "" } else { "s" });
        if !self.renamed.is_empty() {
            let list: Vec<String> = self.renamed.iter().map(|(a, b)| format!("{a} -> {b}")).collect();
            s.push_str(&format!("; {} renamed ({})", self.renamed.len(), list.join(", ")));
        }
        if !self.skipped.is_empty() {
            s.push_str(&format!("; {} already in the document ({})", self.skipped.len(), self.skipped.join(", ")));
        }
        s
    }
}

/// Add every new tool of a library in one step; undo removes exactly
/// those tools and nothing else.
pub struct ImportTools {
    tools: Vec<(String, Tool)>,
    report: ImportReport,
}

impl ImportTools {
    /// Plan the import against the document. Refuses (nothing applied)
    /// a library naming setup furniture, a tool `is_sane` refuses, or a
    /// library whose every tool is already in the document.
    pub fn new(world: &World, library: Vec<LibraryTool>) -> Result<Self, CommandError> {
        let mut tools: Vec<(String, Tool)> = Vec::with_capacity(library.len());
        let mut report = ImportReport::default();
        for LibraryTool { name, tool } in library {
            if let Some(err) = reserved(world, &name) {
                return Err(err);
            }
            if !tool.is_sane() {
                return Err(CommandError::Invalid(format!(
                    "tool '{name}': parameters must be finite and positive"
                )));
            }
            if tools.iter().any(|(n, _)| *n == name) {
                return Err(CommandError::Invalid(format!("tool '{name}' appears twice in the library")));
            }
            let taken = |n: &str| model::find_by_name(world, n).is_some() || tools.iter().any(|(t, _)| t == n);
            match model::find_by_name(world, &name) {
                None if !taken(&name) => {
                    report.added.push(name.clone());
                    tools.push((name, tool));
                }
                Some(e) if model::tool_of(world, &e).as_ref() == Some(&tool) => report.skipped.push(name),
                _ => {
                    let Some(free) = (2..=9999u32).map(|i| format!("{name} ({i})")).find(|c| !taken(c)) else {
                        return Err(CommandError::NameConflict(name));
                    };
                    report.renamed.push((name, free.clone()));
                    tools.push((free, tool));
                }
            }
        }
        if tools.is_empty() {
            return Err(CommandError::Invalid(
                "nothing to import: every tool in the library is already in the document".into(),
            ));
        }
        Ok(ImportTools { tools, report })
    }

    #[must_use]
    pub fn report(&self) -> &ImportReport {
        &self.report
    }
}

impl Command for ImportTools {
    fn apply(&self, world: &World) {
        for (name, tool) in &self.tools {
            let entity = match model::find_by_name(world, name) {
                Some(entity) => entity,
                None => model::spawn(world, name),
            };
            entity.entity_view(world).set(ToolDef { tool: tool.clone(), number: None });
        }
    }

    fn invert(&self, world: &World) {
        for (name, _) in &self.tools {
            if let Some(entity) = model::find_by_name(world, name) {
                model::remove(world, &entity);
            }
        }
    }
}
