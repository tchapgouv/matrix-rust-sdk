use std::{
    env,
    fs::File,
    io::{Seek, SeekFrom},
    path::PathBuf,
    process::exit,
    sync::Arc,
};
use tokio::sync::Mutex;

use matrix_sdk::{
    self, async_trait,
    events::{
        room::message::{MessageEventContent, MessageType, TextMessageEventContent},
        SyncMessageEvent,
    },
    room::Room,
    Client, EventHandler, SyncSettings,
};
use url::Url;

struct ImageBot {
    image: Arc<Mutex<File>>,
}

impl ImageBot {
    pub fn new(image: File) -> Self {
        let image = Arc::new(Mutex::new(image));
        Self { image }
    }
}

#[async_trait]
impl EventHandler for ImageBot {
    async fn on_room_message(&self, room: Room, event: &SyncMessageEvent<MessageEventContent>) {
        if let Room::Joined(room) = room {
            let msg_body = if let SyncMessageEvent {
                content:
                    MessageEventContent {
                        msgtype: MessageType::Text(TextMessageEventContent { body: msg_body, .. }),
                        ..
                    },
                ..
            } = event
            {
                msg_body
            } else {
                return;
            };

            if msg_body.contains("!image") {
                println!("sending image");
                let mut image = self.image.lock().await;

                room.send_attachment("cat", &mime::IMAGE_JPEG, &mut *image, None)
                    .await
                    .unwrap();

                image.seek(SeekFrom::Start(0)).unwrap();

                println!("message sent");
            }
        }
    }
}

async fn login_and_sync(
    homeserver_url: String,
    username: String,
    password: String,
    image: File,
) -> Result<(), matrix_sdk::Error> {
    let homeserver_url = Url::parse(&homeserver_url).expect("Couldn't parse the homeserver URL");
    let client = Client::new(homeserver_url).unwrap();

    client
        .login(&username, &password, None, Some("command bot"))
        .await?;

    client.sync_once(SyncSettings::default()).await.unwrap();
    client
        .set_event_handler(Box::new(ImageBot::new(image)))
        .await;

    let settings = SyncSettings::default().token(client.sync_token().await.unwrap());
    client.sync(settings).await;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), matrix_sdk::Error> {
    tracing_subscriber::fmt::init();
    let (homeserver_url, username, password, image_path) = match (
        env::args().nth(1),
        env::args().nth(2),
        env::args().nth(3),
        env::args().nth(4),
    ) {
        (Some(a), Some(b), Some(c), Some(d)) => (a, b, c, d),
        _ => {
            eprintln!(
                "Usage: {} <homeserver_url> <username> <password> <image>",
                env::args().next().unwrap()
            );
            exit(1)
        }
    };

    println!(
        "helloooo {} {} {} {:#?}",
        homeserver_url, username, password, image_path
    );
    let path = PathBuf::from(image_path);
    let image = File::open(path).expect("Can't open image file.");

    login_and_sync(homeserver_url, username, password, image).await?;
    Ok(())
}
