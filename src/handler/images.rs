use crate::errors::{Error, Result};
use crate::structs;
use crate::structs::reply::{Reply, ReplyType};
use crate::DB;

use humantime::format_duration;
use image::io::Reader;
use img_hash::{HashAlg, HasherConfig, ImageHash};
use log::{error, info, warn};
use phf::phf_set;
use serenity::model::prelude::Message;
use std::io::Cursor;
use std::time::Duration;
use std::time::Instant;

static IGNORED_PROVIDERS: phf::Set<&'static str> = phf_set! {
    "Tenor",
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
    if bytes.len() > 0 {
        Ok(Some(get_image_hash(&bytes)?))
    } else {
        info!("received url with 0 bytes, can't process");
        Ok(None)
    }
}

fn get_duration(msg: &Message, link: &structs::Message) -> Result<Duration> {
    let ret = msg
        .id
        .created_at()
        .signed_duration_since(link.created_at)
        .to_std();
    match ret {
        Ok(val) => Ok(val),
        Err(why) => {
            error!("Failed to get duration for msg (created at: {}) on message id {} (created at: {}) with following error: {why:?}", link.created_at, msg.id, msg.id.created_at());
            Err(Error::Internal(format!("{:?}", why)))
        }
    }
}
fn repost_text(msg: &Message, link: &structs::Message) -> String {
    let duration_text = match get_duration(msg, link) {
        Ok(val) => format_duration(val).to_string(),
        Err(_) => "".to_string(),
    };

    format!("{} {}", duration_text, link.uri())
}

fn repost_message<'a>(msg: &'a Message, reposts: &[structs::Message]) -> Option<Reply<'a>> {
    if !reposts.is_empty() {
        let repost_str = if reposts.len() > 1 {
            format!(
                "\n{}",
                reposts
                    .iter()
                    .map(|x| repost_text(msg, x))
                    .collect::<Vec<String>>()
                    .join("\n")
            )
        } else {
            repost_text(msg, &reposts[0])
        };

        Some(Reply::new(
            format!("🚨 IMAGE 🚨 REPOST 🚨 {repost_str}"),
            ReplyType::Message(msg),
        ))
    } else {
        None
    }
}

pub async fn store_images(msg: &Message, include_reply: bool) -> Result<Option<Reply<'_>>> {
    let msg_id = *msg.id.as_u64();
    let mut hashes = Vec::new();
    if msg.attachments.len() > 0 {
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

    if msg.embeds.len() > 0 {
        info!("msg {msg_id} has {} embeds", msg.embeds.len());
    }
    for embed in &msg.embeds {
        if let Some(provider) = &embed.provider {
            if let Some(provider_name) = &provider.name {
                if IGNORED_PROVIDERS.contains(&provider_name) {
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
    let mut reposts = Vec::new();
    for (hash, url) in hashes {
        if include_reply {
            let b64 = hash.to_base64();
            let matches = db.hash_matches(&b64, *msg.guild_id.unwrap().as_u64())?;
            info!(
                "for {msg_id} with has {b64} found {} matches",
                matches.len()
            );

            for (db_msg, db_hash_b64) in &matches {
                if let Ok(db_hash) = ImageHash::from_base64(&db_hash_b64) {
                    let distance = hash.dist(&db_hash);
                    info!("Hamming Distance for db_hash {db_hash_b64} is {distance}");
                    if distance < 5 {
                        reposts.push(*db_msg);
                    }
                }
            }
        }
        DB::db_call(|db| db.insert_image(url, &hash.to_base64(), msg_id))?;
    }

    if reposts.len() > 0 {
        Ok(repost_message(msg, &reposts))
    } else {
        Ok(None)
    }
}
