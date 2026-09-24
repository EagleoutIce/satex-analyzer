//! File contents that stand in for what is on disk: `satex lint --fix`
//! applies its edits here, reruns the analysis on them, and only writes the
//! files once the fixes have settled.  The interpreter runs on one thread,
//! so the overlay is that thread's.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

thread_local! {
    static FILES: RefCell<HashMap<PathBuf, String>> = RefCell::new(HashMap::new());
}

pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    match FILES.with(|files| files.borrow().get(path).cloned()) {
        Some(text) => Ok(text),
        None => std::fs::read_to_string(path),
    }
}

pub fn set(path: &Path, text: String) {
    FILES.with(|files| files.borrow_mut().insert(path.to_path_buf(), text));
}

pub fn clear() {
    FILES.with(|files| files.borrow_mut().clear());
}
