use crate::errors::{Result, Error};

use chrono::{DateTime, Utc};
use std::time::Instant;
use log::{debug, error, info, trace, warn};
use std::collections::BTreeMap;
use std::sync::RwLock;
use db::{writable_db_call, WriteableDb};


static AUTHOR_CACHE: RwLock<BTreeMap<u64, DateTime<Utc>>> = RwLock::new(BTreeMap::new());

// All TTL in seconds
const AUTHOR_TTL: i64 = 60 * 60 * 3; // 3h

#[inline(always)]
fn check_cache(cache: &RwLock<BTreeMap<u64, DateTime<Utc>>>, id: u64, ttl: i64) ->Result<bool> {
    match cache.read() {
        Ok(cache) => Ok(cache.get(&id).map_or(false, |last_updated| {
            Utc::now().signed_duration_since(*last_updated).num_seconds() < ttl
        })),
        Err(_why) => Err(Error::ConstStr("Failed to acquire read on cache"))
    }
    
}

#[inline(always)]
fn update_cache(cache: &RwLock<BTreeMap<u64, DateTime<Utc>>>, id: u64) -> Result<()> {
    match cache.write() {
        Ok(mut writable_cache) => {
            writable_cache.insert(id, Utc::now());
            Ok(())
        },
        Err(_why) => Err(Error::ConstStr("Failed to acquire write lock on cache"))
    }
}


pub fn update_author(user_id: u64, username: &str, bot: bool, discriminator: u16) -> Result<()>{
    let now = Instant::now();
    
    if !check_cache(&AUTHOR_CACHE, user_id, AUTHOR_TTL)? {
        debug!("author {user_id} not in cache");
        writable_db_call(|db| db.add_user(user_id, username, bot, discriminator))?;
        update_cache(&AUTHOR_CACHE, user_id)?;
    } else {
        debug!("author {user_id} in cache");
    }
   

    debug!(
        "update_author time elapsed: {:.2?}",
        now.elapsed()
    );

    Ok(())
}