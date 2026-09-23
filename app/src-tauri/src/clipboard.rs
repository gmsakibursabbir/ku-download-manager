//! Event-driven clipboard monitoring (no polling): the OS notifies us of
//! clipboard changes; we only react to text that looks like a download.

use clipboard_master::{CallbackResult, ClipboardHandler, Master};
use kucore::classify::{self, Route};
use kucore::{Core, CoreEvent};
use std::sync::Arc;

struct Handler {
    core: Arc<Core>,
    last: String,
}

impl Handler {
    fn check(&mut self) {
        let s = self.core.settings();
        if !s.clipboard_monitor {
            return;
        }
        let Ok(mut cb) = arboard::Clipboard::new() else { return };
        let Ok(text) = cb.get_text() else { return };
        let text = text.trim();
        if text.len() > 4096 || text.contains(char::is_whitespace) || text == self.last {
            return;
        }
        self.last = text.to_string();
        if !classify::scheme_allowed(text) {
            return;
        }
        // Only offer links that are clearly downloads or media, not every web page.
        if classify::classify(text, &s.categories) == Route::Unknown {
            return;
        }
        self.core.emit(CoreEvent::ClipboardUrl { url: text.to_string() });
    }
}

impl ClipboardHandler for Handler {
    fn on_clipboard_change(&mut self) -> CallbackResult {
        self.check();
        CallbackResult::Next
    }

    fn on_clipboard_error(&mut self, error: std::io::Error) -> CallbackResult {
        tracing::debug!("clipboard monitor: {error}");
        CallbackResult::Next
    }
}

pub fn start(core: Arc<Core>) {
    std::thread::Builder::new()
        .name("clipboard".into())
        .spawn(move || match Master::new(Handler { core, last: String::new() }) {
            Ok(mut m) => {
                if let Err(e) = m.run() {
                    tracing::warn!("clipboard monitor stopped: {e}");
                }
            }
            Err(e) => tracing::warn!("clipboard monitor unavailable: {e}"),
        })
        .ok();
}

pub fn read_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}
