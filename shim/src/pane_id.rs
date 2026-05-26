//! tmux pane-id parsing.
//!
//! tmux exposes pane ids in the form `%<n>` (e.g. `%17`). Claude Code's
//! `TmuxBackend` passes these to us verbatim in `-t` arguments — we
//! strip the `%` and parse the numeric id, which is what the plugin
//! tracks under `State::teammates`.
//!
//! Other tmux target forms exist (`<session>:<window>.<pane>`) but
//! `TmuxBackend` never emits them — it always captures `#{pane_id}` and
//! uses the `%N` form for subsequent calls. We reject anything else
//! with a clear error rather than silently mishandling it.

use std::fmt;

/// Parsed numeric pane id from a tmux `%N` target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneId(pub u32);

impl PaneId {
    /// Render back to the tmux wire form (`%<n>`), for printing on
    /// stdout to satisfy `-F '#{pane_id}'`.
    pub fn render(self) -> String {
        format!("%{}", self.0)
    }
}

impl fmt::Display for PaneId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "%{}", self.0)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PaneIdParseError {
    MissingPrefix,
    InvalidNumber,
}

impl fmt::Display for PaneIdParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingPrefix => {
                write!(f, "tmux pane id must start with '%' (e.g. %17)")
            }
            Self::InvalidNumber => write!(f, "tmux pane id digits not a valid u32"),
        }
    }
}

impl std::error::Error for PaneIdParseError {}

/// Parse a `%N` target into a numeric pane id.
///
/// Accepts only the `%<u32>` shape; the `<session>:<window>.<pane>`
/// shape (which `TmuxBackend` never emits) is rejected as
/// `MissingPrefix`.
pub fn parse(s: &str) -> Result<PaneId, PaneIdParseError> {
    let digits = s.strip_prefix('%').ok_or(PaneIdParseError::MissingPrefix)?;
    let n: u32 = digits.parse().map_err(|_| PaneIdParseError::InvalidNumber)?;
    Ok(PaneId(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_id() {
        assert_eq!(parse("%17"), Ok(PaneId(17)));
    }

    #[test]
    fn parses_zero() {
        // Zellij pane ids can start at 0 — we don't ascribe semantic
        // meaning to the value, only the shape.
        assert_eq!(parse("%0"), Ok(PaneId(0)));
    }

    #[test]
    fn rejects_missing_percent() {
        assert_eq!(parse("17"), Err(PaneIdParseError::MissingPrefix));
    }

    #[test]
    fn rejects_non_numeric() {
        assert_eq!(parse("%abc"), Err(PaneIdParseError::InvalidNumber));
    }

    #[test]
    fn rejects_empty_digits() {
        assert_eq!(parse("%"), Err(PaneIdParseError::InvalidNumber));
    }

    #[test]
    fn rejects_session_window_pane_form() {
        // `sess:0.1` — the verbose target form. We don't accept this
        // because TmuxBackend never produces it; if it ever started
        // to, we want a loud error rather than a silent miscast.
        assert_eq!(parse("sess:0.1"), Err(PaneIdParseError::MissingPrefix));
    }

    #[test]
    fn rejects_negative() {
        // u32 parse rejects leading '-'.
        assert_eq!(parse("%-1"), Err(PaneIdParseError::InvalidNumber));
    }

    #[test]
    fn rejects_overflow() {
        // u32::MAX + 1 — parse fails.
        assert_eq!(parse("%4294967296"), Err(PaneIdParseError::InvalidNumber));
    }

    #[test]
    fn renders_back_to_percent_form() {
        assert_eq!(PaneId(17).render(), "%17");
        assert_eq!(PaneId(0).render(), "%0");
        assert_eq!(format!("{}", PaneId(42)), "%42");
    }
}
