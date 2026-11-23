#[macro_use]
pub(crate) mod names;
pub mod color;
pub mod equiv;
pub mod keys;
pub mod progress;
pub mod timing;

/// `count word`, with an `s` unless the count is one ("1 file", "3 files").
pub(crate) fn plural(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}
