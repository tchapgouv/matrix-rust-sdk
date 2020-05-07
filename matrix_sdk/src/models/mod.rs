mod event_deser;
#[cfg(feature = "messages")]
#[cfg_attr(docsrs, doc(cfg(feature = "messages")))]
mod message;
mod room;
mod room_member;

pub use room::{Room, RoomName};
pub use room_member::RoomMember;

#[allow(dead_code)]
pub type Token = String;
