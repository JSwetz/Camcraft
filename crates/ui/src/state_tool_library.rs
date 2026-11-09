//! The Tool library page's Export / Import library buttons (#300): a
//! file dialog, then `tool_library::export_tool_library` to write, or
//! `parse_tool_library` + `ImportTools` (one undo unit) to read. A
//! refusal lands on the Tool pane by name, never swallowed.
//! Provenance: our own `camcraft-tools/1` format (app::tool_library); no
//! vendor tool-library format consulted.

use std::path::{Path, PathBuf};

use camcraft_app::commands::ImportTools;
use camcraft_app::tool_library::{self, EXTENSION};

use super::{ErrorScope, State};

impl State {
    /// "Export library…": ask where, then write every document tool.
    pub(crate) fn export_tool_library_dialog(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("CamCraft Tool Library", &[EXTENSION])
            .set_file_name(format!("tools.{EXTENSION}"))
            .save_file()
        else {
            return;
        };
        self.export_tool_library_to(&tool_library::with_library_extension(&path));
    }

    /// "Import library…": pick a file, then import it.
    pub(crate) fn import_tool_library_dialog(&mut self) {
        let picked: Option<PathBuf> = rfd::FileDialog::new()
            .add_filter("CamCraft Tool Library", &[EXTENSION, "json"])
            .pick_file();
        if let Some(path) = picked {
            self.import_tool_library_from(&path);
        }
    }

    pub(crate) fn export_tool_library_to(&mut self, path: &Path) {
        let written = tool_library::export_tool_library(&self.world)
            .and_then(|text| std::fs::write(path, text).map_err(|e| e.to_string()));
        match written {
            Ok(()) => self.status = format!("tool library written to {}", path.display()),
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool library not exported: {err}")),
        }
    }

    pub(crate) fn import_tool_library_from(&mut self, path: &Path) {
        let planned = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|text| tool_library::parse_tool_library(&text))
            .and_then(|lib| ImportTools::new(&self.world, lib).map_err(|e| e.to_string()));
        match planned {
            Ok(command) => {
                let summary = command.report().summary();
                self.history.execute(&self.world, command);
                self.doc_version += 1;
                self.status = summary;
            }
            Err(err) => self.refuse(ErrorScope::Tool, format!("tool library not imported: {err}")),
        }
    }
}
