use std::env;
use std::process::exit;

use url::Url;

use matrix_sdk::media::{MediaFormat, MediaRequest};
use matrix_sdk::ruma::events::room::MediaSource;
use matrix_sdk::ruma::OwnedMxcUri;
use matrix_sdk::Client;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let Some(homeserver_url) = env::args().nth(1) else {
        eprintln!("Usage: {} <homeserver_url> <mediasource_url>", env::args().next().unwrap());
        exit(1)
    };
    let Some(mediasource_url) = env::args().nth(2) else {
        eprintln!("Usage: {} <homeserver_url> <mediasource_url>", env::args().next().unwrap());
        exit(1)
    };

    download_media(homeserver_url, mediasource_url).await?;

    Ok(())
}

async fn download_media(homeserver_url: String, mediasource_url: String) -> matrix_sdk::Result<Vec<u8>> {
    let homeserver_url = Url::parse(&homeserver_url)?;
    let client = Client::new(homeserver_url).await?;

    let source = MediaSource::Plain(OwnedMxcUri::from(mediasource_url));
    let result = client
        .media()
        .get_media_content(&MediaRequest { source, format: MediaFormat::File }, true)
        .await;

    result
}
