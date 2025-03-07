use std::{env, process::exit};

use matrix_sdk::{
    media::{MediaFormat, MediaRequestParameters},
    ruma::{events::room::MediaSource, OwnedMxcUri},
    Client,
};
use url::Url;

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

async fn download_media(
    homeserver_url: String,
    mediasource_url: String,
) -> matrix_sdk::Result<Vec<u8>> {
    let homeserver_url = Url::parse(&homeserver_url)?;
    let client = Client::new(homeserver_url).await.unwrap();

    let source = MediaSource::Plain(OwnedMxcUri::from(mediasource_url));
    return client
        .media()
        .get_media_content(&MediaRequestParameters { source, format: MediaFormat::File }, true)
        .await;
}
