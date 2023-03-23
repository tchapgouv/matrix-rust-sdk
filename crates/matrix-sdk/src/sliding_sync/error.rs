//! Sliding Sync errors.

use thiserror::Error;

/// Internal representation of errors in Sliding Sync.
#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
    /// The response we've received from the server can't be parsed or doesn't
    /// match up with the current expectations on the client side. A
    /// `sync`-restart might be required.
    #[error("The sliding sync response could not be handled: {0}")]
    BadResponse(String),
    /// Called `.build()` on a builder type, but the given required field was
    /// missing.
    #[error("Required field missing: `{0}`")]
    BuildMissingField(&'static str),
    /// A `SlidingSyncListRequestGenerator` has been used without having been
    /// initialized. It happens when a response is handled before a request has
    /// been sent. It usually happens when testing.
    #[error("The sliding sync list `{0}` is handling a response, but its request generator has not been initialized")]
    RequestGeneratorHasNotBeenInitialized(String),
}
