use anyhow::Result;
use clap::Parser;
use futures_util::{pin_mut, StreamExt};
use log::{debug, info};
use matrix_sdk::encryption::backups::BackupState;
use matrix_sdk::encryption::secret_storage::SecretStore;
use matrix_sdk::media::MediaEventContent;
use matrix_sdk::ruma::events::room::message::MessageType::{Image, Text, Video};
use matrix_sdk::ruma::events::room::message::{SyncRoomMessageEvent, TextMessageEventContent};
use matrix_sdk::ruma::events::SyncMessageLikeEvent::Original;
use matrix_sdk::ruma::{OwnedEventId, RoomId};
use matrix_sdk::{async_trait, config::SyncSettings, ruma::OwnedRoomId, Client, Room};
use matrix_sdk_base_bwi::content_scanner::scan_state::BWIScanState;
use matrix_sdk_bwi::content_scanner::BWIContentScanner;
use matrix_sdk_ui::timeline::TimelineItemContent::Message;
use matrix_sdk_ui::timeline::TimelineItemKind::Virtual;
use matrix_sdk_ui::timeline::VirtualTimelineItem::ScanStateChanged;
use matrix_sdk_ui::timeline::{RoomExt, TimelineUniqueId};
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::time;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;
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
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_filter(filter_fn(|meta| meta.target() == BWI_TARGET)),
        )
        .init();

    let cli = Cli::parse();
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

    println!("Initial timeline items: {timeline_items:#?}");
    tokio::spawn(async move {
        debug!(target: BWI_TARGET, "Timeline handler start for room {room_id:?}");
        while let Some(diff) = timeline_stream.next().await {
            info!(target: BWI_TARGET, "received diff: {diff:?}");
        }
    });

    // Sync forever
    client.sync(sync_settings).await?;

    Ok(())
}

fn is_room_id_matching(room_id: &String, room: &Room) -> bool {
    room.room_id().as_str() == room_id
}
