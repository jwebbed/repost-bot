use crate::errors::{Error, Result};

use chrono::{DateTime, Utc};
use db:: WriteableDb;
use log::debug;
use serenity::model::channel;
use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Instant;

static AUTHOR_CACHE: RwLock<BTreeMap<u64, DateTime<Utc>>> = RwLock::new(BTreeMap::new());
static SERVER_CACHE: RwLock<BTreeMap<u64, DateTime<Utc>>> = RwLock::new(BTreeMap::new());
static CHANNEL_CACHE: RwLock<BTreeMap<u64, DateTime<Utc>>> = RwLock::new(BTreeMap::new());

// All TTL in seconds
const AUTHOR_TTL: i64 = 60 * 60 * 3; // 3h
const SERVER_TTL: i64 = 60 * 60 * 24; // 24h
const CHANNEL_TTL: i64 = 60 * 60 * 6; // 9h


struct MetadataProcessor {

}

fn check_cache(cache: &RwLock<BTreeMap<u64, DateTime<Utc>>>, id: u64, ttl: i64) -> Result<bool> {
    match cache.read() {
        Ok(cache) => Ok(cache.get(&id).map_or(false, |last_updated| {
            Utc::now()
                .signed_duration_since(*last_updated)
                .num_seconds()
                < ttl
        })),
        Err(_why) => Err(Error::ConstStr("Failed to acquire read on cache")),
    }
}

fn update_cache(
    cache: &RwLock<BTreeMap<u64, DateTime<Utc>>>,
    id: u64,
) -> Result<Option<DateTime<Utc>>> {
    match cache.write() {
        Ok(mut writable_cache) => Ok(writable_cache.insert(id, Utc::now())),
        Err(_why) => Err(Error::ConstStr("Failed to acquire write lock on cache")),
    }
}

pub fn update_author(
    db: &impl WriteableDb,
    user_id: u64,
    username: &str,
    bot: bool,
    discriminator: u16,
) -> Result<()> {
   // let now = Instant::now();

    if !check_cache(&AUTHOR_CACHE, user_id, AUTHOR_TTL)? {
       // debug!("author {user_id} not in cache");
        // writable_db_call(|db| db.add_user(user_id, username, bot, discriminator))?;
        db.add_user(user_id, username, bot, discriminator)?;
        update_cache(&AUTHOR_CACHE, user_id)?;
    } else {
       // debug!("author {user_id} in cache");
    }

  //  debug!("update_author time elapsed: {:.2?}", now.elapsed());

    Ok(())
}

pub fn update_server(db: &impl WriteableDb, server_id: u64, name: &Option<String>) -> Result<()> {
   // let now = Instant::now();

    if !check_cache(&SERVER_CACHE, server_id, SERVER_TTL)? {
       // debug!("server {server_id} not in cache");
         db.update_server(server_id, name)?;
        update_cache(&SERVER_CACHE, server_id)?;
    } else {
      //  debug!("server {server_id} in cache");
    }

   // debug!("update_server time elapsed: {:.2?}", now.elapsed());

    Ok(())
}

pub fn update_channel(db: &impl WriteableDb, channel_id: u64, server_id: u64, name: &str, visible: bool) -> Result<()> {
  //  let now = Instant::now();

    if !check_cache(&CHANNEL_CACHE, channel_id, CHANNEL_TTL)? {
    //    debug!("channel {channel_id} not in cache");
        db.update_channel(channel_id, server_id, name, visible)?;
        update_cache(&CHANNEL_CACHE, channel_id)?;
    } else {
     //   debug!("channel {channel_id} in cache");
    }

  //  debug!("update_channel time elapsed: {:.2?}", now.elapsed());

    Ok(())
}
