use crate::errors::{Error, Result};
use crate::structs::repost::{RepostSet, RepostType};

use db::{get_read_only_db, writable_db_call, ReadOnlyDb, WriteableDb};
use image::error::ImageError;
use image::io::Reader;
use log::{info, warn};
use phf::phf_set;
use serenity::model::channel::{Attachment, Embed};
use serenity::model::prelude::Message;
use std::io::Cursor;
use std::time::Instant;
use visual_hash::{HashAlg, HasherConfig, ImageHash};

static IGNORED_PROVIDERS: phf::Set<&'static str> = phf_set! {
    "Tenor",
    "YouTube",
};

// Primarily a seperate function for testing purposes
fn hash_img(image: &image::DynamicImage) -> ImageHash {
    HasherConfig::new()
        .hash_alg(HashAlg::Gradient)
        .hash_size(16, 16)
        .to_hasher()
        .hash_image(image)
}

fn get_image_hash(bytes: &Vec<u8>) -> Result<Option<ImageHash>> {
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

async fn download_and_hash(url: &str, proxy_url: Option<&String>) -> Result<Option<ImageHash>> {
    let req_url = proxy_url.map_or(url, |u| u);
    let bytes = reqwest::get(req_url).await?.bytes().await?.to_vec();
    if !bytes.is_empty() {
        Ok(get_image_hash(&bytes)?)
    } else {
        info!("received url with 0 bytes, can't process");
        Ok(None)
    }
}

#[derive(Debug)]
pub struct ImageProcesser<'a> {
    msg_id: u64,
    server_id: u64,
    attachments: &'a Vec<Attachment>,
    embeds: &'a Vec<Embed>,
}

impl<'a> ImageProcesser<'a> {
    pub const fn new(
        msg_id: u64,
        server_id: u64,
        attachments: &'a Vec<Attachment>,
        embeds: &'a Vec<Embed>,
    ) -> ImageProcesser<'a> {
        ImageProcesser {
            msg_id,
            server_id,
            attachments,
            embeds,
        }
    }
}

impl ImageProcesser<'_> {
    pub fn from_message(msg: &Message) -> Result<ImageProcesser<'_>> {
        Ok(ImageProcesser::new(
            *msg.id.as_u64(),
            *msg.guild_id.ok_or(Error::ConstStr("idk"))?.as_u64(),
            &msg.attachments,
            &msg.embeds,
        ))
    }

    pub async fn process(&self, include_reply: bool) -> Result<RepostSet> {
        store_images_direct(
            self.msg_id,
            self.server_id,
            self.attachments,
            self.embeds,
            include_reply,
        )
        .await
    }
}

async fn store_images_direct<'a>(
    msg_id: u64,
    server_id: u64,
    attachments: &'a Vec<Attachment>,
    embeds: &'a Vec<Embed>,
    include_reply: bool,
) -> Result<RepostSet> {
    let mut hashes = Vec::new();
    if !attachments.is_empty() {
        info!("msg {msg_id} has {} attachments", attachments.len());
    }
    for attachment in attachments {
        if attachment
            .content_type
            .as_ref()
            .map_or(true, |t| !t.starts_with("image"))
        {
            continue;
        }
        let download_time = Instant::now();
        // need to actually handle download failures at some pointc
        let bytes = attachment.download().await?;
        warn!(
            "msg {msg_id} has attachment with {} bytes downloaded in {:.2?}",
            bytes.len(),
            download_time.elapsed()
        );
        let parse_time = Instant::now();
        if let Some(hash) = get_image_hash(&bytes)? {
            warn!(
                "msg {msg_id} has attachment with hash {} parsed in {:.2?}",
                hash.to_base64(),
                parse_time.elapsed()
            );
            hashes.push((hash, &attachment.url));
        }
    }

    if !embeds.is_empty() {
        info!("msg {msg_id} has {} embeds", embeds.len());
    }
    for embed in embeds {
        if let Some(provider) = &embed.provider {
            if let Some(provider_name) = &provider.name {
                if IGNORED_PROVIDERS.contains(provider_name) {
                    info!("provider {provider_name} is ignored, skipping this embed");
                    continue;
                }
                info!("provider {provider_name} is not ignored, processing");
            }
        }

        if let Some(embedi) = &embed.image {
            info!("msg {msg_id} found image embed");
            if let Some(hash) = download_and_hash(&embedi.url, embedi.proxy_url.as_ref()).await? {
                hashes.push((hash, &embedi.url));
            }
        } else if let Some(embedi) = &embed.thumbnail {
            info!("msg {msg_id} found thumbnail embed");
            if let Some(hash) = download_and_hash(&embedi.url, embedi.proxy_url.as_ref()).await? {
                hashes.push((hash, &embedi.url));
            }
        }
    }
    let db = get_read_only_db()?;
    let mut reposts = RepostSet::new();
    for (hash, url) in hashes {
        if include_reply {
            let b64 = hash.to_base64();
            let matches = db.hash_matches(&b64, server_id, msg_id)?;
            info!(
                "for {msg_id} with has {b64} found {} matches",
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
        writable_db_call(|mut db| db.insert_image(url, &hash.to_base64(), msg_id))?;
    }
    Ok(reposts)
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
