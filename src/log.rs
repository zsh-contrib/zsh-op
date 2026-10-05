use std::fmt::Display;

/// Prints a warning to stderr.
pub fn warn(message: impl Display) {
    eprintln!("zsh-op: warning: {message}");
}

/// Prints an informational message to stderr.
pub fn info(message: impl Display) {
    eprintln!("zsh-op: {message}");
}
