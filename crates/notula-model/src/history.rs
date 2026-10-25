use crate::command::Command;
use crate::error::ModelError;
use crate::ids::EventId;
use crate::score::{Clef, Part, Score};

/// A score together with its undo/redo history; all edits go through `execute`.
#[derive(Debug, Clone, Default)]
pub struct Document {
    score: Score,
    undo_stack: Vec<Command>,
    redo_stack: Vec<Command>,
    revision: u64,
}

impl Document {
    pub fn new() -> Self {
        Self::default()
    }

    /// Wraps a loaded score with an empty history.
    pub fn from_score(score: Score) -> Self {
        Self {
            score,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            revision: 0,
        }
    }

    /// Changes whenever the score does, including through undo and redo.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn score(&self) -> &Score {
        &self.score
    }

    pub fn into_score(self) -> Score {
        self.score
    }

    pub fn execute(&mut self, command: Command) -> Result<(), ModelError> {
        let inverse = command.apply(&mut self.score)?;
        self.undo_stack.push(inverse);
        self.redo_stack.clear();
        self.revision += 1;
        Ok(())
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Returns `false` when there was nothing to undo.
    pub fn undo(&mut self) -> Result<bool, ModelError> {
        let Some(inverse) = self.undo_stack.pop() else {
            return Ok(false);
        };
        match inverse.clone().apply(&mut self.score) {
            Ok(redo) => {
                self.redo_stack.push(redo);
                self.revision += 1;
                Ok(true)
            }
            Err(error) => {
                self.undo_stack.push(inverse);
                Err(error)
            }
        }
    }

    /// Returns `false` when there was nothing to redo.
    pub fn redo(&mut self) -> Result<bool, ModelError> {
        let Some(command) = self.redo_stack.pop() else {
            return Ok(false);
        };
        match command.clone().apply(&mut self.score) {
            Ok(undo) => {
                self.undo_stack.push(undo);
                self.revision += 1;
                Ok(true)
            }
            Err(error) => {
                self.redo_stack.push(command);
                Err(error)
            }
        }
    }

    // Id allocation is not an edit, so it bypasses the history.
    pub fn new_event_id(&mut self) -> EventId {
        self.score.new_event_id()
    }

    pub fn new_part(&mut self, name: &str, clefs: &[Clef]) -> Part {
        self.score.new_part(name, clefs)
    }
}
