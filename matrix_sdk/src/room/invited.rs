use std::ops::Deref;

use crate::{room::Common, BaseRoom, Client, Result, RoomType};

/// A room in the invited state.
///
/// This struct contains all methods specific to a `Room` with type
/// `RoomType::Invited`. Operations may fail once the underlying `Room` changes
/// `RoomType`.
#[derive(Debug, Clone)]
pub struct Invited {
    pub(crate) inner: Common,
}

impl Invited {
    /// Create a new `room::Invited` if the underlying `Room` has type
    /// `RoomType::Invited`.
    ///
    /// # Arguments
    /// * `client` - The client used to make requests.
    ///
    /// * `room` - The underlying room.
    pub fn new(client: Client, room: BaseRoom) -> Option<Self> {
        // TODO: Make this private
        if room.room_type() == RoomType::Invited {
            Some(Self { inner: Common::new(client, room) })
        } else {
            None
        }
    }

    /// Reject the invitation.
    pub async fn reject_invitation(&self) -> Result<()> {
        self.inner.leave().await
    }

    /// Accept the invitation.
    pub async fn accept_invitation(&self) -> Result<()> {
        self.inner.join().await
    }
}

impl Deref for Invited {
    type Target = Common;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
