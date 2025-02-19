use crate::errors::Result;
use crate::structs::repost::{RepostSet, RepostType};
use crate::structs::{AttachmentType, Post, PostProcessor, ProcessedPost};

use db::{get_read_only_db, writable_db_call, ReadOnlyDb, WriteableDb};
use image::error::ImageError;
use image::io::Reader;
use log::{info, warn};
use phf::phf_set;
use std::io::Cursor;
use std::sync::Arc;
use std::time::Instant;
use visual_hash::{HashAlg, HasherConfig, ImageHash};

static IGNORED_PROVIDERS: phf::Set<&'static str> = phf_set! {
    "Tenor",
    "YouTube",
    "Apple Music",
};

#[derive(Debug)]
pub struct ImageProcessor {
    post: Post,
}

struct HashedImages {
    db_message: db::structs::Message,
    hashes: Vec<(ImageHash, Arc<str>)>,
}

impl PostProcessor for ImageProcessor {
    fn new(post: Post) -> ImageProcessor {
        ImageProcessor { post }
    }

    async fn process(&self) -> Result<impl ProcessedPost> {
        if !self.post.has_attachments() {
            return Ok(None);
        }
        let mut hashes = Vec::new();
        for attachment in self.post.attachments() {
            if should_process(&attachment.attachment_type) {
                // need to actually handle download failures at some pointc
                let bytes = attachment.download().await?;
                let parse_time = Instant::now();
                if let Some(hash) = get_image_hash(bytes)? {
                    warn!(
                        "msg {} has attachment with hash {} parsed in {:.2?}",
                        self.post.id(),
                        hash.to_base64(),
                        parse_time.elapsed()
                    );
                    hashes.push((hash, attachment.url.clone()));
                }
            }
        }
        Ok(Some(HashedImages {
            db_message: self.post.db_message,
            hashes,
        }))
    }
}

impl ProcessedPost for Option<HashedImages> {
    fn get_reposts(&self) -> Result<RepostSet> {
        let mut reposts = RepostSet::new();
        if let Some(HashedImages { db_message, hashes }) = self {
            let db = get_read_only_db()?;
            for (hash, _url) in hashes {
                let b64 = hash.to_base64();
                let matches = db.hash_matches(&b64, db_message.server, db_message.id)?;
                info!(
                    "for {} with has {b64} found {} matches",
                    db_message.id,
                    matches.len()
                );

                for (db_msg, db_hash_b64) in &matches {
                    if let Ok(db_hash) = ImageHash::from_base64(db_hash_b64) {
                        let distance = hash.dist(&db_hash);
                        info!("Hamming Distance for db_hash {db_hash_b64} is {distance}");
                        if distance < 5 {
                            reposts.add(*db_msg, RepostType::Image);
                        }
                    }
                }
            }
        }

        Ok(reposts)
    }
    fn store_post(&self) -> Result<()> {
        if let Some(HashedImages { db_message, hashes }) = self {
            for (hash, url) in hashes {
                writable_db_call(|mut db| db.insert_image(url, &hash.to_base64(), db_message.id))?;
            }
        }
        Ok(())
    }
}

// Primarily a seperate function for testing purposes
fn hash_img(image: &image::DynamicImage) -> ImageHash {
    HasherConfig::new()
        .hash_alg(HashAlg::Gradient)
        .hash_size(16, 16)
        .to_hasher()
        .hash_image(image)
}

fn get_image_hash(bytes: &[u8]) -> Result<Option<ImageHash>> {
    let image = Reader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode();
    // decoding error we likely can't do anything about, should just log and ignore
    if let Err(err) = &image {
        if let ImageError::Decoding(_) = err {
            warn!("decoding error occured, skipping {err:?}");
            return Ok(None);
        }
    }
    Ok(Some(hash_img(&image?)))
}

fn should_process(attachment_type: &AttachmentType) -> bool {
    // Match block in order so the order is intentionally set to the most to least specific.
    match attachment_type {
        AttachmentType::Attachment { content_type } => content_type
            .as_ref()
            .map_or(false, |t| t.starts_with("image")),

        AttachmentType::EmbedImage {
            provider_name: Some(provider_name),
        } => should_process_provider(provider_name),
        AttachmentType::EmbedImage {
            provider_name: None,
        } => true,

        // Experimentally it seems that, with threads, all profile images are of article "link" and other images are
        // of kind "article". This may exclude some embeds that are valid reposts, but that seems unlikely.
        AttachmentType::EmbedThumbnail {
            provider_name: Some(provider_name),
            square_dimension: Some(dimension),
            is_link_type: true,
        } => **provider_name != *"Threads" || *dimension > 640,
        AttachmentType::EmbedThumbnail {
            provider_name: Some(provider_name),
            ..
        } => should_process_provider(provider_name),
        AttachmentType::EmbedThumbnail {
            provider_name: None,
            ..
        } => true,
    }
}

fn should_process_provider(provider_name: &str) -> bool {
    !IGNORED_PROVIDERS.contains(provider_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    macro_rules! image_hash_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (file_name, expected_hash) = $value;
                let root_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
                let image = image::open(format!("{root_dir}/test_resources/{file_name}")).unwrap();
                assert_eq!(
                    expected_hash,
                    &hash_img(&image).to_base64()
                );

            }
        )*
        }
    }

    // These tests primarily exist to identify if something changes in the underlying
    // visual_hash library, to identify that it still hashs known images the way we expect
    // it too. Further it should also fail on any changes we make to how we use said lib

    // TODO: Should add non-jpeg file formats to ensure matching across file formats
    image_hash_tests! {
        photo1_large: ("photo1_large.jpg", "MuNy4INik8O0wRjlGjZNdmlPbA9kO9f50/hDek3aRcY="),
        photo1_med:   ("photo1_med.jpg",   "MuNy4INik8O0wRjlGjZNdmlPbA9kO9f50/hDek3aRcY="),
        photo1_small: ("photo1_small.jpg", "MuNy4INik8O0wRjlGjZNdmlPbA9kO9f50/hDek3aRcY="),
        photo2_large: ("photo2_large.jpg", "2YXmlWYDvQiN0M7Gfw7ZPNi0mB2QKbF7MLYn5QEvAXM="),
        photo2_med:   ("photo2_med.jpg",   "2YXmlWYDvQiN0M7Gfw7ZPNi0mB2QKbF7MLYn5QEvAXM="),
        photo2_small: ("photo2_small.jpg", "2YXmlWYDvQiN0M7Gfw7ZPNi0mB2QKbF7MLYn5QEvAXM="),
        photo2_xs:    ("photo2_xs.jpg",    "2YXmlWYDvQiN0M7Gfw7ZPNi0mB2QKbF7MLYn5QEvAXM="),
    }
}
