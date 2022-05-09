use crate::errors::Result;
use crate::structs::reply::Reply;
use crate::structs::repost::{RepostSet, RepostType};
use crate::DB;

use image::io::Reader;
use img_hash::{HashAlg, HasherConfig, ImageHash};
use log::{info, warn};
use phf::phf_set;
use serenity::model::prelude::Message;
use std::io::Cursor;
use std::time::Instant;

static IGNORED_PROVIDERS: phf::Set<&'static str> = phf_set! {
    "Tenor",
    "YouTube",
};

fn get_image_hash(bytes: &Vec<u8>) -> Result<ImageHash> {
    let image = Reader::new(Cursor::new(bytes))
        .with_guessed_format()?
        .decode()?;
    Ok(HasherConfig::new()
        .hash_alg(HashAlg::Gradient)
        .hash_size(16, 16)
        .to_hasher()
        .hash_image(&image))
}

async fn download_and_hash(url: &str, proxy_url: Option<&String>) -> Result<Option<ImageHash>> {
    let req_url = match proxy_url {
        Some(u) => u,
        None => url,
    };
    let bytes = reqwest::get(req_url).await?.bytes().await?.to_vec();
    if !bytes.is_empty() {
        Ok(Some(get_image_hash(&bytes)?))
    } else {
        info!("received url with 0 bytes, can't process");
        Ok(None)
    }
}

pub async fn store_images(msg: &Message, include_reply: bool) -> Result<Option<Reply<'_>>> {
    let msg_id = *msg.id.as_u64();
    let mut hashes = Vec::new();
    if !msg.attachments.is_empty() {
        info!("msg {msg_id} has {} attachments", msg.attachments.len());
    }
    for attachment in &msg.attachments {
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
        let hash = get_image_hash(&bytes)?;
        warn!(
            "msg {msg_id} has attachment with hash {} parsed in {:.2?}",
            hash.to_base64(),
            parse_time.elapsed()
        );

        hashes.push((hash, &attachment.url));
    }

    if !msg.embeds.is_empty() {
        info!("msg {msg_id} has {} embeds", msg.embeds.len());
    }
    for embed in &msg.embeds {
        if let Some(provider) = &embed.provider {
            if let Some(provider_name) = &provider.name {
                if IGNORED_PROVIDERS.contains(provider_name) {
                    info!("provider {provider_name} is ignored, skipping this embed");
                    continue;
                }
            }
        }

        if let Some(embedi) = &embed.image {
            info!("msg {msg_id} found image embed in msg {msg:?}");
            if let Some(hash) = download_and_hash(&embedi.url, embedi.proxy_url.as_ref()).await? {
                hashes.push((hash, &embedi.url));
            }
        } else if let Some(embedi) = &embed.thumbnail {
            info!("msg {msg_id} found thumbnail embed in msg {msg:?}");
            if let Some(hash) = download_and_hash(&embedi.url, embedi.proxy_url.as_ref()).await? {
                hashes.push((hash, &embedi.url));
            }
        }
    }
    let db = DB::get_db()?;
    let reposts = RepostSet::new();
    for (hash, url) in hashes {
        if include_reply {
            let b64 = hash.to_base64();
            let matches = db.hash_matches(&b64, *msg.guild_id.unwrap().as_u64())?;
            info!(
                "for {msg_id} with has {b64} found {} matches",
                matches.len()
            );

            for (db_msg, db_hash_b64) in &matches {
                if let Ok(db_hash) = ImageHash::from_base64(db_hash_b64) {
                    let distance = hash.dist(&db_hash);
                    info!("Hamming Distance for db_hash {db_hash_b64} is {distance}");
                    if distance < 5 {
                        reposts.add(RepostType::Image, *db_msg);
                    }
                }
            }
        }
        DB::db_call(|db| db.insert_image(url, &hash.to_base64(), msg_id))?;
    }

    if reposts.len() > 0 {
        Ok(reposts.generate_reply_for_message(msg))
    } else {
        Ok(None)
    }
}
