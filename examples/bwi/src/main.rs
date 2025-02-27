use anyhow::Result;
use clap::Parser;
use eyeball_im::VectorDiff;
use futures_util::{pin_mut, StreamExt};
use log::{debug, info};
use matrix_sdk::attachment::AttachmentConfig;
use matrix_sdk::encryption::backups::BackupState;
use matrix_sdk::encryption::secret_storage::SecretStore;
use matrix_sdk::media::{MediaEventContent, MediaFormat, MediaRequestParameters};
use matrix_sdk::ruma::events::room::message::{ImageMessageEventContent, MessageType};
use matrix_sdk::ruma::push::ComparisonOperator::Le;
use matrix_sdk::{config::SyncSettings, ruma::OwnedRoomId, Client, Room};
use matrix_sdk_ui::timeline::{RoomExt, TimelineItem, TimelineItemContent, TimelineItemKind};
use std::fs;
use std::path::{absolute, Path};
use std::sync::Arc;
use tracing::Level;
use tracing_subscriber::filter::{filter_fn, FilterExt};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{filter, Layer};
use url::Url;

const BWI_TARGET: &str = "BWI";

#[derive(Parser, Debug)]
struct Cli {
    /// The homeserver to connect to.
    #[clap(value_parser)]
    homeserver: Url,

    /// The user name that should be used for the login.
    #[clap(value_parser)]
    user_name: String,

    /// The password that should be used for the login.
    #[clap(value_parser)]
    password: String,

    /// Set the proxy that should be used for the connection.
    #[clap(short, long)]
    proxy: Option<Url>,

    /// Enable verbose logging output.
    #[clap(short, long, action)]
    verbose: bool,

    /// The room id that we should listen for the,
    #[clap(value_parser)]
    room_id: String,

    #[clap(long, action)]
    secret_store_key: String,
}

async fn login(cli: &Cli) -> Result<Client> {
    // Note that when encryption is enabled, you should use a persistent store to be
    // able to restore the session with a working encryption setup.
    // See the `persist_session` example.
    let mut builder =
        Client::builder().homeserver_url(&cli.homeserver).without_server_jwt_token_validation();

    if let Some(proxy) = &cli.proxy {
        builder = builder.proxy(proxy);
    }

    let client = builder.build().await?;

    client
        .matrix_auth()
        .login_username(&cli.user_name, &cli.password)
        .initial_device_display_name("rust-sdk")
        .await?;

    Ok(client)
}

async fn import_known_secrets(client: &Client, secret_store: SecretStore) -> Result<()> {
    secret_store.import_secrets().await?;

    let status = client
        .encryption()
        .cross_signing_status()
        .await
        .expect("We should be able to get our cross-signing status");

    if status.is_complete() {
        println!("Successfully imported all the cross-signing keys");
    } else {
        eprintln!("Couldn't import all the cross-signing keys: {status:?}");
    }

    Ok(())
}

async fn listen_for_backup_state_changes(client: Client) {
    let stream = client.encryption().backups().state_stream();
    pin_mut!(stream);

    while let Some(state) = stream.next().await {
        let Ok(state) = state else { panic!("Error while receiving backup state updates") };

        match state {
            BackupState::Unknown => (),
            BackupState::Enabling => println!("Trying to enable backups"),
            BackupState::Resuming => println!("Trying to resume backups"),
            BackupState::Enabled => println!("Backups have been successfully enabled"),
            BackupState::Downloading => println!("Downloading the room keys from the backup"),
            BackupState::Disabling => println!("Disabling the backup"),
            BackupState::Creating => println!("Trying to create a new backup"),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    setup_logging(cli.verbose);

    let room_id = cli.room_id.clone();
    let client = login(&cli).await?;

    let sync_settings = SyncSettings::default();

    // Wait for the first sync response
    println!("Wait for the first sync");

    client.sync_once(sync_settings.clone()).await?;

    let secret_store =
        client.encryption().secret_storage().open_secret_store(&cli.secret_store_key).await?;

    let _task = tokio::spawn({
        let client = client.clone();
        async move { listen_for_backup_state_changes(client).await }
    });

    import_known_secrets(&client, secret_store).await?;

    let available_rooms = client.rooms();

    available_rooms.iter().for_each(|room| {
        println!("Room with id {} and name {}", room.room_id(), room.name().unwrap())
    });

    let room_id =
        {
            if available_rooms.iter().any(|room| is_room_id_matching(&room_id, room)) {
                OwnedRoomId::try_from(room_id.as_str())?
            } else {
                available_rooms
                    .iter()
                    .find(|room| {
                        if let Some(room_name) = room.name() {
                            room_name == room_id
                        } else {
                            false
                        }
                    })
                    .expect("No room with that name")
                    .room_id()
                    .to_owned()
            }
        };

    // Get the timeline stream and listen to it.
    println!("Try to connect to room with id: {}", &room_id);
    let room = client.get_room(&room_id).unwrap();
    let timeline = room.timeline().await?;
    timeline.setup_content_scanner_hook_ext().await;

    let (timeline_items, mut timeline_stream) = timeline.subscribe().await;

    let room_id = room.room_id().to_owned();

    let client_copy = client.clone();

    println!("Initial timeline items: {timeline_items:#?}");
    tokio::spawn(async move {
        debug!(target: BWI_TARGET, "Timeline handler start for room {room_id:?}");
        while let Some(diff) = timeline_stream.next().await {
            info!(target: BWI_TARGET, "received diff: {diff:?}");
            match diff {
                VectorDiff::PushFront { value } => handle_timeline_item(&value, &client_copy).await,
                VectorDiff::PushBack { value } => handle_timeline_item(&value, &client_copy).await,
                VectorDiff::Insert { index: _, value } => {
                    handle_timeline_item(&value, &client_copy).await
                }
                VectorDiff::Set { index: _, value } => {
                    handle_timeline_item(&value, &client_copy).await
                }
                _ => {}
            }
        }
    });

    tokio::spawn(async move {
        for _ in 0..5 {
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            send_image(&room).await;
        }
    });

    // Sync forever
    client.sync(sync_settings).await?;

    Ok(())
}

async fn send_image(room: &Room) {
    let image_path = Path::new("./examples/bwi/ressources/image.png");
    let image = fs::read(&image_path).expect("Can't open image file.");
    room.send_attachment(
        "image.png",
        &mime::IMAGE_PNG,
        image,
        AttachmentConfig::new().caption(Some("my pretty cat".to_owned())),
    )
    .await
    .expect("###BWI### could not send image");
}

fn setup_logging(verbose: bool) {
    println!("with verbose logging {:?}", verbose);
    if verbose {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_filter(filter::LevelFilter::from_level(Level::DEBUG)),
            )
            .init();
    } else {
        tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_filter(filter_fn(|meta| meta.target() == BWI_TARGET))
                    .with_filter(filter::LevelFilter::from_level(Level::INFO)),
            )
            .init();
    }
}

async fn handle_timeline_item(item: &Arc<TimelineItem>, _client: &Client) {
    if let TimelineItemKind::Event(e) = item.kind() {
        if let TimelineItemContent::Message(m) = e.content() {
            match m.msgtype() {
                MessageType::Image(_content) => {}
                // MessageType::Image(content) => handle_image_content(content, client).await,
                _ => {}
            }
        }
    }
}

async fn handle_image_content(content: &ImageMessageEventContent, client: &Client) {
    let request =
        MediaRequestParameters { source: content.source().unwrap(), format: MediaFormat::File };
    let file_handle = client
        .media()
        .get_media_file(&request, None, &(mime::IMAGE_PNG), false, Some("./".to_string()))
        .await;
    let path = Path::new("./foo.png");
    info!(target: BWI_TARGET, "{:?}", absolute(path));
    file_handle.unwrap().persist(path).unwrap();
}

fn is_room_id_matching(room_id: &String, room: &Room) -> bool {
    room.room_id().as_str() == room_id
}
