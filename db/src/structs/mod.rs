mod link;
mod message;
pub mod wordle;

pub use link::Channel;
pub use link::Link;
pub use message::Message;

#[derive(Debug)]
pub struct Reply {
    pub id: u64,
    pub channel: u64,
    pub replied_to: u64,
}

#[derive(Debug, Default)]
pub struct RepostCount {
    pub link: String,
    pub count: u64,
}

#[derive(Debug, Default)]
pub struct ReposterCount {
    pub username: String,
    pub count: u64,
}
