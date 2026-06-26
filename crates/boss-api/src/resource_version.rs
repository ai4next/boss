use serde::{Deserialize, Serialize};

/// Monotonic, store-assigned revision. Used for optimistic concurrency and
/// as the cursor for list/watch.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(transparent)]
pub struct ResourceVersion(pub u64);

impl ResourceVersion {
    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for ResourceVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ResourceVersion {
    type Err = std::num::ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse::<u64>()?))
    }
}
