//! A setup's own post options and its tool-list export (#389). The post
//! page's option widgets (skip pauses and homes, Output, threading) queue
//! the same `CommandKind`s as before; they land on the ACTIVE setup's own
//! options (`SetSetupPostOptions`) when it owns them and on the document's
//! (`SetPostOptions`) when it follows - the `state::setup_mount` routing
//! law, worn by the options. Every arm is one undoable command. The tool
//! list export is a read of the document plus a file write: no command,
//! no history (the setup-sheet law).
//! Provenance: our own routing law (issue #389); no vendor material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::commands::{CommandError, SetPostOptions, SetSetupPostOptions};
use camcraft_app::schema::PostOptions;
use camcraft_app::{setup, setup_post_options, tool_list_export};

use super::{ErrorScope, State};

impl State {
    /// The active setup when it owns its post options: the setup the
    /// option widgets edit. `None` - they edit the document's.
    #[must_use]
    pub fn post_options_owner(&self) -> Option<String> {
        let active = setup::active(&self.world)?;
        setup_post_options::own(&self.world, Some(&active)).map(|_| active)
    }

    /// Set the post options in force: the active setup's own when it has
    /// them, else the document's. `done` is the status on success.
    pub(super) fn route_post_options(&mut self, next: PostOptions, done: &str, refused: &str, scope: ErrorScope) {
        let owner = self.post_options_owner();
        let executed: Result<(), CommandError> = match &owner {
            Some(s) => SetSetupPostOptions::new(&self.world, s, Some(next)).map(|c| self.history.execute(&self.world, c)),
            None => SetPostOptions::new(&self.world, next).map(|c| self.history.execute(&self.world, c)),
        };
        match executed {
            Ok(()) => {
                self.doc_version += 1;
                self.status = match owner {
                    Some(s) => format!("{done} ({s})"),
                    None => done.to_owned(),
                };
            }
            Err(err) => self.refuse(scope, format!("{refused}: {err}")),
        }
    }

    /// A setup takes its own post options (a copy of the ones it posts
    /// with now, so nothing it posts changes) or follows the document's.
    pub(super) fn own_post_options(&mut self, setup: &str, own: bool) {
        let next = own.then(|| setup_post_options::of(&self.world, Some(setup)));
        match SetSetupPostOptions::new(&self.world, setup, next) {
            Ok(c) => {
                self.history.execute(&self.world, c);
                self.doc_version += 1;
                self.status = if own {
                    format!("{setup} has its own post options")
                } else {
                    format!("{setup} follows the document's post options")
                };
            }
            Err(err) => self.refuse(ErrorScope::Setup, format!("post options not changed: {err}")),
        }
    }

    /// Ask where to write `setup`'s tool list, then write it.
    pub(super) fn tool_list_export_dialog(&mut self, setup: Option<String>) {
        let stem = setup.clone().or_else(|| setup::effective(&self.world, None)).unwrap_or_else(|| "setup".to_owned());
        let picked: Option<PathBuf> = rfd::FileDialog::new()
            .add_filter("CSV", &["csv"])
            .add_filter("Text", &["txt"])
            .set_file_name(format!("{stem}-tools.csv"))
            .save_file();
        if let Some(path) = picked {
            self.tool_list_export_path(setup.as_deref(), &path);
        }
    }

    /// Write `setup`'s tool list to `path`: CSV for `.csv`, text
    /// otherwise. A pure read of the document; the status line is the
    /// only effect.
    pub(super) fn tool_list_export_path(&mut self, setup: Option<&str>, path: &Path) {
        let text = tool_list_export::for_path(&self.world, setup, path);
        let count = tool_list_export::rows(&self.world, setup).len();
        match std::fs::write(path, text) {
            Ok(()) => self.status = format!("tool list ({count} tool(s)) written to {}", path.display()),
            Err(err) => self.status = format!("tool list write failed: {err}"),
        }
    }
}
