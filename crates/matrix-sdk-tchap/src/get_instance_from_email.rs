use reqwest::{self, Client};
use serde::Deserialize;
use thiserror::Error;
use tokio::runtime::Builder;
use url::Url;
use tracing::error;

//--------------------------------------------------------------------------------
// Definitions
//--------------------------------------------------------------------------------
#[derive(Debug, Clone, uniffi::Record)]
pub struct TchapGetInstanceConfig {
    pub home_server: String,
    pub user_agent: String,
}

impl TchapGetInstanceConfig {
    // `impl` needed to generate uniffi init() for the struct (iOS side at least).
    pub fn new(home_server: String) -> Self {
        Self { home_server: home_server, user_agent: "Tchap-rust-default-user-agent".to_string() }
    }
}

impl Default for TchapGetInstanceConfig {
    fn default() -> Self {
        Self {
            home_server: "agent.dinum.tchap.gouv.fr".to_string(),
            user_agent: "Tchap-rust-default-user-agent".to_string(),
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
    home_server: String,
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
            Ok(client) => Self { home_server: config.home_server.to_owned(), client: Some(client) },
        }
    }

    fn build_client(with_user_agent: &str) -> Result<Client, TchapGetInstanceError> {
        let builder = Client::builder()
            .user_agent(with_user_agent);

        #[cfg(target_os = "android")]
        let builder = Self::android_setup_tls(builder)?;

        builder
            .build()
            .map_err(|_| TchapGetInstanceError::NoClient)
    }

    #[cfg(target_os = "android")]
    fn android_setup_tls(builder: reqwest::ClientBuilder) -> Result<reqwest::ClientBuilder, TchapGetInstanceError> {
        use rustls::RootCertStore;
        use rustls::client::WebPkiServerVerifier;
        use std::sync::Arc;
        use tracing::warn;

        let mut root_store = RootCertStore::empty();

        // Add webpki_roots to ensure Harica CRL-only certs work on older Android devices (API 33 and below)
        root_store.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

        // Also load the native certs
        let native_certs = rustls_native_certs::load_native_certs().certs;
        root_store.add_parsable_certificates(native_certs);

        let verifier = WebPkiServerVerifier::builder(Arc::new(root_store))
            .build()
            .map_err(|error| {
                warn!("WebPkiServerVerifier build error: {}", error);
                TchapGetInstanceError::NoClient
            })?;

        let config = rustls::ClientConfig::builder()
            .with_webpki_verifier(verifier)
            .with_no_client_auth();

        Ok(builder.use_preconfigured_tls(config))
    }

    /// Construct the full url from the email requested.
    fn url(&self, for_email: &str) -> Result<Url, url::ParseError> {
        // Check if the homeserver starts with "https://matrix.", else add it.
        let home_server_address = if self.home_server.starts_with("https://matrix.") {
            // Clean the trailing '/' if any.
            self.home_server.clone().strip_suffix('/').unwrap_or(&self.home_server).to_owned()
        } else {
            format!("https://matrix.{}", self.home_server)
        };
        let kmxidentity_apiprefix_path_v1 = "_matrix/identity/api/v1";
        let info_path_and_query = format!("info?medium=email&address={}", for_email);
        Url::parse(
            format!(
                "{}/{}/{}",
                home_server_address, kmxidentity_apiprefix_path_v1, info_path_and_query
            )
            .as_str(),
        )
    }

    /// Request the backend for the HomeServer associated with the given email, if possible.
    // #[tokio::main]
    pub fn get_instance(
        &self,
        for_email: String,
    ) -> Result<TchapGetInstanceResult, TchapGetInstanceError> {
        let cloned_client = self.client.clone();
        let url = self.url(&for_email);

        // Create the runtime
        let runtime = Builder::new_multi_thread()
            .enable_time()
            .enable_io()
            .build()
            .expect("Can't create runtime");

        // Spawn a future onto the runtime
        runtime.block_on(async move {
            // Check if the client is available.
            if let Some(client) = &cloned_client {
                match url {
                    // If the request URL can't be build, return an error.
                    Err(error) => {
                        error!("error to build the request when get_instance for an email: {error}");
                        Err(TchapGetInstanceError::BadUrl)
                    },
                    Ok(request_url) => {
                        // The client is available and the URL is built, request the backend.
                        match client
                            .get(request_url)
                            .send()
                            .await
                            .map_err(|error| {
                                error!("error to receive the response when get_instance for an email: {error}");
                                TchapGetInstanceError::InvalidResult
                            })?
                            .json::<TchapGetInstanceResult>()
                            .await
                        {
                            // If the request failed, return an error.
                            Err(error) => {
                                error!("error to receive the response when get_instance for an email: {error}");
                                Err(TchapGetInstanceError::InvalidResult)
                            },
                            // Else, return the value from the backend.
                            Ok(value) => Ok(value),
                        }
                    }
                }
            } else {
                // If the client is not available, return an error.
                Err(TchapGetInstanceError::NoClient)
            }
        })
    }
}

//--------------------------------------------------------------------------------
// Tests
//--------------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use crate::get_instance_from_email::*;

    #[test]
    fn test_instance_lookup_from_email() {
        let config = TchapGetInstanceConfig::default();
        let client = TchapGetInstance::new(&config);

        assert_eq!(
            "agent.dinum.tchap.gouv.fr",
            client.get_instance("testeur@dinum.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.tchap.gouv.fr",
            client.get_instance("testeur@agent.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.agriculture.tchap.gouv.fr",
            client.get_instance("testeur@agriculture.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.diplomatie.tchap.gouv.fr",
            client.get_instance("testeur@diplomatie.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.pm.tchap.gouv.fr",
            client.get_instance("testeur@pm.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.elysee.tchap.gouv.fr",
            client.get_instance("testeur@elysee.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.finances.tchap.gouv.fr",
            client.get_instance("testeur@finances.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.interieur.tchap.gouv.fr",
            client.get_instance("testeur@interieur.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.intradef.tchap.gouv.fr",
            client.get_instance("testeur@intradef.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.justice.tchap.gouv.fr",
            client.get_instance("testeur@justice.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.dev-durable.tchap.gouv.fr",
            client.get_instance("testeur@dev-durable.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.social.tchap.gouv.fr",
            client.get_instance("testeur@social.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.education.tchap.gouv.fr",
            client.get_instance("testeur@education.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.culture.tchap.gouv.fr",
            client.get_instance("testeur@culture.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.collectivites.tchap.gouv.fr",
            client.get_instance("testeur@collectivites.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.externe.tchap.gouv.fr",
            client.get_instance("testeur@externe.tchap.beta.gouv.fr".to_string()).unwrap().hs
        );
        assert_eq!(
            "agent.externe.tchap.gouv.fr",
            client.get_instance("testeur@gmail.com".to_string()).unwrap().hs
        );
    }
}
