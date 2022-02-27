pub mod link;
pub mod reply;
pub mod wordle;

pub use link::Link;
pub use link::Message;

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
