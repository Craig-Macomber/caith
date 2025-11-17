use std::ops::Deref;

/// Used to mark a dice roll if its result is a critic.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum Critic {
    /// Normal result
    No,
    /// Minimum reached
    Min,
    /// Maximum reached
    Max,
}
