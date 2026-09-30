use crate::errors::Result;
use crate::structs::repost::{RepostSet, RepostType};
use crate::structs::{Attachment, AttachmentType, Post, PostProcessor, ProcessedPost};

use db::{ReadOnlyDb, WriteableDb, get_read_only_db, writable_db_call};
use futures_util::future::join_all;
use image::io::Reader;
use log::{info, warn};
use phf::phf_set;
use std::io::Cursor;
use std::sync::{Arc, LazyLock};
use std::time::Instant;
use visual_hash::{HashAlg, Hasher, HasherConfig, ImageHash};

static IGNORED_PROVIDERS: phf::Set<&'static str> = phf_set! {
    "Tenor",
    "YouTube",
    "Apple Music",
};

static HASHER: LazyLock<Hasher> = LazyLock::new(|| {
    HasherConfig::new()
        .hash_alg(HashAlg::Gradient)
        .hash_size(16, 16)
        .to_hasher()
});

/// Images with a hamming distance below this are considered the same image
const MATCH_DISTANCE_THRESHOLD: u32 = 5;

#[derive(Debug)]
pub struct ImageProcessor;

pub struct HashedImages {
    db_message: db::structs::Message,
    hashes: Vec<(ImageHash, Arc<str>)>,
}

impl PostProcessor for ImageProcessor {
    type Processed = HashedImages;

    async fn process(post: &Post) -> Result<HashedImages> {
        // download and hash every image concurrently
        let hashes = join_all(
            post.attachments()
                .filter(|attachment| should_process(&attachment.attachment_type))
                .map(|attachment| hash_attachment(post.id(), attachment)),
        )
        .await
        .into_iter()
        .filter_map(Result::transpose)
        .collect::<Result<_>>()?;

        Ok(HashedImages {
            db_message: post.db_message,
            hashes,
        })
    }
}

/// Downloads and hashes an attachment. Attachments that can't be downloaded or
/// decoded are logged and skipped, otherwise the message would never be marked
/// as processed and would be retried forever.
async fn hash_attachment(
    msg_id: u64,
    attachment: &Attachment,
) -> Result<Option<(ImageHash, Arc<str>)>> {
    let bytes = match attachment.download().await {
        Ok(bytes) => bytes,
        Err(why) => {
            warn!(
                "msg {msg_id} failed to download attachment {}, skipping: {why:?}",
                attachment.url
            );
            return Ok(None);
        }
    };
    let parse_time = Instant::now();
    // decoding and hashing is cpu heavy so keep it off the async workers
    let hash = tokio::task::spawn_blocking(move || get_image_hash(&bytes)).await?;
    Ok(hash.map(|hash| {
        info!(
            "msg {msg_id} has attachment with hash {} parsed in {:.2?}",
            hash.to_base64(),
            parse_time.elapsed()
        );
        (hash, attachment.url.clone())
    }))
}

impl ProcessedPost for HashedImages {
    fn get_reposts(&self) -> Result<RepostSet> {
        let mut reposts = RepostSet::default();
        if self.hashes.is_empty() {
            return Ok(reposts);
        }

        let db = get_read_only_db()?;
        for (hash, _url) in &self.hashes {
            let b64 = hash.to_base64();
            let matches = db.hash_matches(&b64, self.db_message.server, self.db_message.id)?;
            info!(
                "for {} with hash {b64} found {} matches",
                self.db_message.id,
                matches.len()
            );

            for (db_msg, db_hash_b64) in &matches {
                if let Ok(db_hash) = ImageHash::from_base64(db_hash_b64) {
                    let distance = hash.dist(&db_hash);
                    info!("Hamming Distance for db_hash {db_hash_b64} is {distance}");
                    if distance < MATCH_DISTANCE_THRESHOLD {
                        reposts.add(*db_msg, RepostType::Image);
                    }
                }
            }
        }

        Ok(reposts)
    }

    fn store_post(&self) -> Result<()> {
        if self.hashes.is_empty() {
            return Ok(());
        }
        let hashes: Vec<(&str, String)> = self
            .hashes
            .iter()
            .map(|(hash, url)| (&**url, hash.to_base64()))
            .collect();
        writable_db_call(|mut db| {
            db.insert_images(
                hashes.iter().map(|(url, hash)| (*url, hash.as_str())),
                self.db_message.id,
            )
        })?;
        Ok(())
    }
}

// Primarily a seperate function for testing purposes
fn hash_img(image: &image::DynamicImage) -> ImageHash {
    HASHER.hash_image(image)
}

/// Returns the hash of an image, or none if the bytes can't be decoded as an
/// image. Decoding errors are expected for untrusted input, so they are logged
/// rather than treated as an error.
fn get_image_hash(bytes: &[u8]) -> Option<ImageHash> {
    let image = Reader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(image::ImageError::IoError)
        .and_then(Reader::decode);
    match image {
        Ok(image) => Some(hash_img(&image)),
        Err(err) => {
            warn!("failed to decode image, skipping {err:?}");
            None
        }
    }
}

fn should_process(attachment_type: &AttachmentType) -> bool {
    // Match block in order so the order is intentionally set to the most to least specific.
    match attachment_type {
        AttachmentType::Attachment { content_type } => content_type
            .as_ref()
            .is_some_and(|t| t.starts_with("image")),

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
        } => &**provider_name != "Threads" || *dimension > 640,
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

    fn test_resource(file_name: &str) -> String {
        let root_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        format!("{root_dir}/test_resources/{file_name}")
    }

    macro_rules! image_hash_tests {
        ($($name:ident: $value:expr,)*) => {
        $(
            #[test]
            fn $name() {
                let (file_name, expected_hash) = $value;
                let image = image::open(test_resource(file_name)).unwrap();
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

    #[test]
    fn test_get_image_hash_from_bytes() {
        let bytes = std::fs::read(test_resource("photo1_small.jpg")).unwrap();
        assert_eq!(
            get_image_hash(&bytes).unwrap().to_base64(),
            "MuNy4INik8O0wRjlGjZNdmlPbA9kO9f50/hDek3aRcY="
        );
    }

    #[test]
    fn test_get_image_hash_png_matches_jpeg() {
        let jpeg = image::open(test_resource("photo2_med.jpg")).unwrap();
        let mut png = Vec::new();
        jpeg.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let jpeg_hash = hash_img(&jpeg);
        let png_hash = get_image_hash(&png).unwrap();
        assert!(jpeg_hash.dist(&png_hash) < MATCH_DISTANCE_THRESHOLD);
    }

    #[test]
    fn test_different_images_dont_match() {
        let photo1 = hash_img(&image::open(test_resource("photo1_small.jpg")).unwrap());
        let photo2 = hash_img(&image::open(test_resource("photo2_small.jpg")).unwrap());
        assert!(photo1.dist(&photo2) >= MATCH_DISTANCE_THRESHOLD);
    }

    #[test]
    fn test_get_image_hash_invalid_data() {
        // unknown format
        assert!(get_image_hash(b"definitely not an image").is_none());
        // known format but corrupt
        let bytes = std::fs::read(test_resource("photo1_small.jpg")).unwrap();
        assert!(get_image_hash(&bytes[..bytes.len() / 8]).is_none());
        assert!(get_image_hash(&[]).is_none());
    }

    #[test]
    fn test_should_process_attachments() {
        let attachment = |content_type: Option<&str>| AttachmentType::Attachment {
            content_type: content_type.map(String::from),
        };
        assert!(should_process(&attachment(Some("image/png"))));
        assert!(should_process(&attachment(Some("image/jpeg"))));
        assert!(!should_process(&attachment(Some("video/mp4"))));
        assert!(!should_process(&attachment(None)));
    }

    #[test]
    fn test_should_process_embed_images() {
        let embed = |provider: Option<&str>| AttachmentType::EmbedImage {
            provider_name: provider.map(Box::from),
        };
        assert!(should_process(&embed(None)));
        assert!(should_process(&embed(Some("Imgur"))));
        assert!(!should_process(&embed(Some("Tenor"))));
        assert!(!should_process(&embed(Some("YouTube"))));
        assert!(!should_process(&embed(Some("Apple Music"))));
    }

    #[test]
    fn test_should_process_embed_thumbnails() {
        let thumbnail = |provider: Option<&str>, dimension: Option<u32>, is_link_type: bool| {
            AttachmentType::EmbedThumbnail {
                provider_name: provider.map(Box::from),
                square_dimension: dimension,
                is_link_type,
            }
        };
        assert!(should_process(&thumbnail(None, None, false)));
        assert!(should_process(&thumbnail(None, Some(100), true)));
        assert!(should_process(&thumbnail(Some("Reddit"), None, false)));
        assert!(!should_process(&thumbnail(Some("YouTube"), None, false)));

        // small square threads link thumbnails are profile pictures
        assert!(!should_process(&thumbnail(
            Some("Threads"),
            Some(320),
            true
        )));
        assert!(should_process(&thumbnail(
            Some("Threads"),
            Some(1080),
            true
        )));
        assert!(should_process(&thumbnail(
            Some("Threads"),
            Some(320),
            false
        )));
        assert!(should_process(&thumbnail(Some("Threads"), None, true)));
    }
}
