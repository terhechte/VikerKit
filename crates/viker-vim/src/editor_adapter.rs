use viker_core::editor::Editor;
use viker_core::input::mode::Mode;

use crate::keymap::KeymapState;

impl KeymapState for Editor {
    fn mode(&self) -> Mode {
        self.mode
    }

    fn pending_keys(&self) -> &[char] {
        &self.pending_keys
    }

    fn clear_pending_keys(&mut self) {
        self.pending_keys.clear();
    }

    fn push_pending_key(&mut self, ch: char) {
        self.pending_keys.push(ch);
    }

    fn count_prefix(&self) -> Option<usize> {
        self.count_prefix
    }

    fn set_count_prefix(&mut self, count: Option<usize>) {
        self.count_prefix = count;
    }

    fn pending_operator_count(&self) -> Option<usize> {
        self.pending_operator_count
    }

    fn set_pending_operator_count(&mut self, count: Option<usize>) {
        self.pending_operator_count = count;
    }

    fn set_selected_register(&mut self, ch: char) {
        self.selected_register = Some(ch);
    }

    fn request_quit(&mut self) {
        self.should_quit = true;
    }

    fn showing_file_finder(&self) -> bool {
        self.showing_file_finder
    }

    fn showing_workspace_symbols(&self) -> bool {
        self.showing_workspace_symbols
    }

    fn showing_hover(&self) -> bool {
        self.showing_hover
    }

    fn dismiss_hover(&mut self) {
        self.showing_hover = false;
        self.hover_text = None;
    }

    fn showing_references(&self) -> bool {
        self.showing_references
    }

    fn showing_code_actions(&self) -> bool {
        self.showing_code_actions
    }

    fn showing_diagnostics(&self) -> bool {
        self.showing_diagnostics
    }

    fn showing_completion(&self) -> bool {
        self.showing_completion
    }

    fn recording_macro(&self) -> bool {
        self.recording_macro.is_some()
    }
}
