mod post;
pub mod reply;
pub mod repost;
pub use post::{AttachmentType, Post};

use crate::errors::Result;

pub trait PostProcessor {
    fn new(post: Post) -> Self;

    async fn process(&self) -> Result<impl ProcessedPost>;
}

pub trait ProcessedPost {
    fn get_reposts(&self) -> Result<repost::RepostSet>;
    fn store_post(&self) -> Result<()>;
}
