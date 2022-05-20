pub mod link;
pub mod message;
pub mod reply;
pub mod repost;
pub mod wordle;

pub use link::Channel;
pub use link::Link;
pub use message::Message;
pub use reply::DbReply;

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
