//! Stage 6: the JS sandbox (Boa, compute-only).
//!
//! Contract: scripts run pre-layout over a data model with a tiny host API.
//! No DOM, no network, no filesystem — ever. Boa lands in M6; M0 ships the
//! evaluation result type.

/// Result of running user scripts over the input document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScriptOutput {
    /// Text nodes produced by scripts, keyed by target element id.
    pub injected: Vec<(String, String)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        assert!(ScriptOutput::default().injected.is_empty());
    }
}
