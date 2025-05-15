use reqwest::{self, Client};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

//--------------------------------------------------------------------------------
// Definitions
//--------------------------------------------------------------------------------
#[derive(Debug, Clone, uniffi::Object)]
pub struct TchapGetInstanceConfig {
    pub home_server: String, // host name without `https://matrix.`
    pub user_agent: String,
}

impl Default for TchapGetInstanceConfig {
    fn default() -> Self {
        Self { 
            home_server: "agent.dinum.tchap.gouv.fr".to_string(), 
            user_agent: "Tchap-rust-default-user-agent".to_string()
         }
    }
}

#[derive(Debug, Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TchapGetInstanceError {
    #[error("Unable to instanciate HTTP client.")]
    NoClient,
    #[error("Unable to access URL.")]
    BadUrl,
    #[error("The returned data is invalid.")]
    InvalidResult,
}

#[derive(Deserialize, Debug, uniffi::Record)]
pub struct TchapGetInstanceResult {
    pub hs: String,
}

#[derive(uniffi::Object)]
pub struct TchapGetInstance {
    home_server: String, // host name without `https://matrix.`
    client: Option<Client>,
}

//--------------------------------------------------------------------------------
// Implementation
//--------------------------------------------------------------------------------
impl TchapGetInstance {
    /// Can return a TchapGetInstance with a None client if an error occured during client initialization.
    /// Error can happend during TLS configuration of the client.
    pub fn new(config: &TchapGetInstanceConfig) -> TchapGetInstance {
        match Self::build_client(&config.user_agent) {
            Err(_) => Self { home_server: config.home_server.to_owned(), client: None },
            Ok(client) => Self { home_server: config.home_server.to_owned(), client: Some(client) }
        }
    }

    fn build_client(with_user_agent: &str) -> Result<Client, TchapGetInstanceError> {
        let client = Client::builder()
        .user_agent(with_user_agent)
        // .use_rustls_tls()
        // .tls_built_in_root_certs(false)
        // .add_root_certificate(cert)
        .build()
        .map_err(|_| TchapGetInstanceError::NoClient);
        return client;
    }

    /// Construct the full url from the email requested.
    fn url(&self, for_email: &str) -> Result<Url, url::ParseError> {
        let home_server_address = format!("https://matrix.{}", self.home_server);
        let kmxidentity_apiprefix_path_v1 = "_matrix/identity/api/v1";
        let info_path_and_query = format!("info?medium=email&address={}", for_email);
        Url::parse(format!("{}/{}/{}", home_server_address, kmxidentity_apiprefix_path_v1, info_path_and_query).as_str())
    }

    /// Request the backend for the HoneServer associated with the given email, if possible.
    pub async fn get_instance(&self, for_email: String) -> Result<TchapGetInstanceResult, TchapGetInstanceError> {
        // CHeck if the client is available.
        if let Some(client) = &self.client {
            match self.url(&for_email) {
                // If the request URL can't be build, return an error.
                Err(_) => Err(TchapGetInstanceError::BadUrl),
                Ok(request_url) => {
                    // The client is available and the URL is built, request the backend.
                    match client.get(request_url)
                        .send()
                        .await
                        .map_err(|_| TchapGetInstanceError::InvalidResult)?
                        .json::<TchapGetInstanceResult>()
                        .await {
                            // If the request failed, return an error.
                            Err(_) => Err(TchapGetInstanceError::InvalidResult),
                            // Else, return the value from the backend.
                            Ok(value) => Ok(value)
                    }
                }
            }
        }
        else {
            // If the client is not available, return an error.
            Err(TchapGetInstanceError::NoClient)
        }
     }
}

//--------------------------------------------------------------------------------
// Tests
//--------------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use crate::get_instance_from_email::*;

    #[tokio::test]
    async fn test_instance_lookup_from_email() {
        let config = TchapGetInstanceConfig::default();
        let client = TchapGetInstance::new(&config);

        assert_eq!("agent.dinum.tchap.gouv.fr", client.get_instance("testeur@dinum.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.tchap.gouv.fr", client.get_instance("testeur@agent.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.agriculture.tchap.gouv.fr", client.get_instance("testeur@agriculture.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.diplomatie.tchap.gouv.fr", client.get_instance("testeur@diplomatie.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.pm.tchap.gouv.fr", client.get_instance("testeur@pm.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.elysee.tchap.gouv.fr", client.get_instance("testeur@elysee.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.finances.tchap.gouv.fr", client.get_instance("testeur@finances.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.interieur.tchap.gouv.fr", client.get_instance("testeur@interieur.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.intradef.tchap.gouv.fr", client.get_instance("testeur@intradef.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.justice.tchap.gouv.fr", client.get_instance("testeur@justice.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.dev-durable.tchap.gouv.fr", client.get_instance("testeur@dev-durable.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.social.tchap.gouv.fr", client.get_instance("testeur@social.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.education.tchap.gouv.fr", client.get_instance("testeur@education.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.culture.tchap.gouv.fr", client.get_instance("testeur@culture.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.collectivites.tchap.gouv.fr", client.get_instance("testeur@collectivites.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.externe.tchap.gouv.fr", client.get_instance("testeur@externe.tchap.beta.gouv.fr".to_string()).await.unwrap().hs);
        assert_eq!("agent.externe.tchap.gouv.fr", client.get_instance("testeur@gmail.com".to_string()).await.unwrap().hs);
    }
}