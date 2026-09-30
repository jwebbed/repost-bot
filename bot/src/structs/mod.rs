mod post;
pub mod reply;
pub mod repost;
pub use post::{Attachment, AttachmentType, Post};

use crate::errors::Result;

pub trait PostProcessor {
    type Processed: ProcessedPost;

    async fn process(post: &Post) -> Result<Self::Processed>;
}

pub trait ProcessedPost {
    fn get_reposts(&self) -> Result<repost::RepostSet>;
    fn store_post(&self) -> Result<()>;
}
