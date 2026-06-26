use boss_common::BossError;

pub type RuntimeError = BossError;
pub type RuntimeResult<T> = std::result::Result<T, RuntimeError>;
