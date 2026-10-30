use std::path::PathBuf;

use notula_model::Document;
use notula_model::clipboard::Clip;
use notula_model::edit::Cursor;
use serde::Serialize;

/// The open document together with where it lives on disk.
pub struct Session {
    pub doc: Document,
    /// The last copied selection; it stays when another score is opened.
    pub clipboard: Option<Clip>,
    path: Option<PathBuf>,
    saved_revision: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
    pub file_name: Option<String>,
    pub title: String,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        let saved_revision = doc.revision();
        Self {
            doc,
            clipboard: None,
            path: None,
            saved_revision,
        }
    }

    /// Replaces the document, e.g. after opening or creating a score.
    pub fn replace(&mut self, doc: Document, path: Option<PathBuf>) {
        self.saved_revision = doc.revision();
        self.doc = doc;
        self.path = path;
    }

    pub fn mark_saved(&mut self, path: PathBuf) {
        self.saved_revision = self.doc.revision();
        self.path = Some(path);
    }

    pub fn path(&self) -> Option<&PathBuf> {
        self.path.as_ref()
    }

    pub fn is_dirty(&self) -> bool {
        self.doc.revision() != self.saved_revision
    }

    pub fn status(&self) -> Status {
        Status {
            can_undo: self.doc.can_undo(),
            can_redo: self.doc.can_redo(),
            dirty: self.is_dirty(),
            file_name: self
                .path
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned()),
            title: self.doc.score().title.clone(),
        }
    }

    pub fn window_title(&self) -> String {
        let status = self.status();
        let name = status
            .file_name
            .or_else(|| (!status.title.is_empty()).then_some(status.title))
            .unwrap_or_else(|| "Untitled".to_owned());
        let marker = if status.dirty { "*" } else { "" };
        format!("{marker}{name} - Notula")
    }

    /// The start of the first staff, where entry begins in a new score.
    pub fn first_cursor(&self) -> Option<Cursor> {
        self.doc.score().staves().next().map(|staff| Cursor {
            staff: staff.id,
            measure: 0,
            voice: 0,
            tick: 0,
        })
    }
}
