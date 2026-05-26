use std::ops::{Deref, DerefMut};

use viker_core::config::Config;
use viker_core::editor::document::Document;
use viker_core::input::mode::Mode;
use viker_core::key::KeyInput;

pub use viker_core::vim::{Effect, LastChange, Register};

pub struct VimCore(viker_core::vim::VimCore);

impl VimCore {
    pub fn from_text(text: &str) -> Self {
        Self(viker_core::vim::VimCore::from_text(text))
    }

    pub fn with_config(document: Document, config: Config) -> Self {
        Self(viker_core::vim::VimCore::with_config(document, config))
    }

    pub fn into_inner(self) -> viker_core::vim::VimCore {
        self.0
    }

    pub fn process_key(&mut self, key: KeyInput) -> Vec<Effect> {
        self.pending_effects.clear();
        if let Some(invocation) = crate::keymap::map_key(self, key) {
            self.execute_invocation(invocation);
        }
        std::mem::take(&mut self.pending_effects)
    }
}

impl Deref for VimCore {
    type Target = viker_core::vim::VimCore;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for VimCore {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl crate::keymap::KeymapState for VimCore {
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
        self.pending_effects.push(Effect::Quit);
    }

    fn recording_macro(&self) -> bool {
        self.recording_macro.is_some()
    }
}
