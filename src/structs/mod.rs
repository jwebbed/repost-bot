mod link;
pub use link::Link;

#[derive(Debug, Default)]
pub struct RepostCount {
    pub link: String,
    pub count: u64,
}
