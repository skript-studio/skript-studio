use std::fmt;

/// Port of Java `ch.njol.skript.patterns.MalformedPatternException`.
#[derive(Debug, Clone)]
pub struct MalformedPatternError {
    /// The full pattern string that failed to compile.
    pub pattern: String,
    /// Human-readable description of the problem.
    pub message: String,
}

impl MalformedPatternError {
    pub fn new(pattern: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            message: message.into(),
        }
    }
}

impl fmt::Display for MalformedPatternError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "malformed pattern {:?}: {}", self.pattern, self.message)
    }
}

impl std::error::Error for MalformedPatternError {}
