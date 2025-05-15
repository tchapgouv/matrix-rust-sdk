use matrix_sdk_tchap::get_instance_from_email::{TchapGetInstance, TchapGetInstanceConfig, TchapGetInstanceError};
use thiserror::Error;

// Throwing errors can't be exported using Uniffi from crate to crate.
// CrateA can't throws (or return a Result using) an Error defined in CrateB.
// Compilation crashes at the end with error: `Unknown throw type`
//
// see: https://github.com/mozilla/uniffi-rs/issues/1605
//
// So, as a workaround, defined a second bridged Error type here,
// and implement `From` traits based on internal error.
 
#[derive(Debug, Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TchapGetInstanceErrorBridged {
    #[error("Unable to instanciate HTTP client.")]
    NoClient,
    #[error("Unable to access URL.")]
    BadUrl,
    #[error("The returned data is invalid.")]
    InvalidResult,
}

impl From<TchapGetInstanceError> for TchapGetInstanceErrorBridged {
    fn from(e: TchapGetInstanceError) -> Self {
        match e {
            TchapGetInstanceError::NoClient => Self::NoClient,
            TchapGetInstanceError::BadUrl => Self::BadUrl,
            TchapGetInstanceError::InvalidResult => Self::InvalidResult,
        }
    }
}

/// Function tchapGetInstance(config, for_email) is used to request backend
/// which homeServer is attributed to an email.
///
/// # Arguments
///
/// * `config` - A struct containing the configuration for the request:
///   * `homeServer`: String - The homeServer to use for the request (e.g. "agent.dinum.tchap.gouv.fr")
///   * `userAgent`: String - The `userAgent` value transmitted with the request)
/// 
/// # Returns
/// A `Result` containing:
///   * if successful: the homeServer to use for this user's email (e.g. "agent.agriculture.tchap.gouv.fr")
///   * If failure: the explanation of the failure
#[uniffi::export]
pub async fn tchap_get_instance(config: &TchapGetInstanceConfig, for_email: String) -> Result<String, TchapGetInstanceErrorBridged> {
   match TchapGetInstance::new(config).get_instance(for_email).await {
    Ok(result) => Ok(result.hs),
    Err(error) => Err(error.into()),
   }
}

#[cfg(test)]
mod tests {
    use matrix_sdk_tchap::get_instance_from_email::TchapGetInstanceConfig;
    use crate::tchap_bindings::tchap_get_instance;
    
    #[tokio::test]
    async fn test_tchap_get_instance() {
        let config = TchapGetInstanceConfig { 
            home_server: "agent.dinum.tchap.gouv.fr".to_string(),
            user_agent: "Tchap-own-user-agent".to_string(),
        };

        assert_eq!("agent.dinum.tchap.gouv.fr", tchap_get_instance(&config, "testeur@dinum.tchap.beta.gouv.fr".to_string()).await.unwrap());
        assert_eq!("agent.externe.tchap.gouv.fr", tchap_get_instance(&config, "testeur@gmail.com".to_string()).await.unwrap());
    }
}