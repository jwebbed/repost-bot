mod link;
pub mod wordle;
pub use link::Link;
pub use link::Message;

#[derive(Debug, Default)]
pub struct RepostCount {
    pub link: String,
    pub count: u64,
}
