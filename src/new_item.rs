use std::path::PathBuf;

use crate::ui::TextInput;
use winit::keyboard::Key;

const MAX_ERROR_CHARS: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewItemKind {
    File,
    Folder,
}

impl NewItemKind {
    pub fn title(self) -> &'static str {
        match self {
            Self::File => "New File",
            Self::Folder => "New Folder",
        }
    }
}

#[derive(Debug)]
pub struct NewItemPrompt {
    pub kind: NewItemKind,
    /// Directory that will receive the new entry.
    pub parent: PathBuf,
    pub input: TextInput,
    pub error: Option<String>,
}

impl NewItemPrompt {
    pub fn new(kind: NewItemKind, parent: PathBuf) -> Self {
        Self {
            kind,
            parent,
            input: TextInput {
                focused: true,
                ..TextInput::default()
            },
            error: None,
        }
    }

    pub fn set_error(&mut self, message: &str) {
        self.error = Some(message.chars().take(MAX_ERROR_CHARS).collect());
    }

    pub fn handle_key(&mut self, key: &Key, text: Option<&str>) -> bool {
        let mut changed = false;
        for edit in TextInput::keys_from_logical_and_text(key, text) {
            changed |= self.input.handle_key(edit);
        }
        if changed {
            self.error = None;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompts_start_empty_and_focused() {
        let file = NewItemPrompt::new(NewItemKind::File, PathBuf::from("/repo"));
        assert!(file.input.text.is_empty());
        assert_eq!(file.input.cursor, 0);
        assert!(file.input.focused);
        let folder = NewItemPrompt::new(NewItemKind::Folder, PathBuf::from("/repo"));
        assert!(folder.input.text.is_empty());
    }

    #[test]
    fn prompt_remembers_its_target_folder() {
        let parent = std::path::PathBuf::from("/repo/src");
        let prompt = NewItemPrompt::new(NewItemKind::File, parent.clone());
        assert_eq!(prompt.parent, parent);
    }

    #[test]
    fn prompt_key_editing_is_bounded_and_clears_errors() {
        use winit::keyboard::{Key, NamedKey};
        let mut prompt = NewItemPrompt::new(NewItemKind::File, PathBuf::from("/repo"));
        prompt.input.text.clear();
        prompt.input.cursor = 0;
        prompt.set_error("old error");
        assert!(prompt.handle_key(&Key::Named(NamedKey::Space), Some(" ")));
        assert!(prompt.handle_key(&Key::Character("x".into()), Some("x")));
        assert_eq!(prompt.input.text, " x");
        assert!(prompt.error.is_none());
        assert!(prompt.handle_key(&Key::Named(NamedKey::Backspace), None));
        assert_eq!(prompt.input.text, " ");
    }

    #[test]
    fn prompt_errors_are_bounded() {
        let mut prompt = NewItemPrompt::new(NewItemKind::File, PathBuf::from("/repo"));
        prompt.set_error(&"x".repeat(1000));
        assert_eq!(prompt.error.as_deref(), Some("x".repeat(160).as_str()));
    }
}
