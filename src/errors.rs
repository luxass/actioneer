use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Update(#[from] crate::cmd::update::Error),
    #[error(transparent)]
    Validate(#[from] crate::cmd::validate::Error),
    #[error(transparent)]
    Audit(#[from] crate::cmd::audit::Error),
}
