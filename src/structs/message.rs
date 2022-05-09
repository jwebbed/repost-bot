use crate::errors::Result;

use chrono::{DateTime, Utc};
use log::debug;
use serenity::model::id::{ChannelId, GuildId, MessageId};
use std::time::Duration;

#[derive(Debug, Copy, Clone)]
pub struct Message {
    // snowflakes referencing the various attributes
    pub id: u64,     // the snowflake of this message
    pub server: u64, // called guilds for some reason in discord API
    pub channel: u64,
    pub author: Option<u64>,

    // time that the message for this link was created
    pub created_at: DateTime<Utc>,

    // flags to indicate if various things were processed
    parsed_repost: Option<DateTime<Utc>>,
    parsed_wordle: Option<DateTime<Utc>>,
    parsed_embed: Option<DateTime<Utc>>,
    deleted: Option<DateTime<Utc>>,
    checked_old: Option<DateTime<Utc>>,
}

impl Message {
    pub const fn new(
        id: u64,
        server: u64,
        channel: u64,
        author: Option<u64>,
        created_at: DateTime<Utc>,
        parsed_repost: Option<DateTime<Utc>>,
        parsed_wordle: Option<DateTime<Utc>>,
        parsed_embed: Option<DateTime<Utc>>,
        deleted: Option<DateTime<Utc>>,
        checked_old: Option<DateTime<Utc>>,
    ) -> Message {
        Message {
            id,
            server,
            channel,
            author,
            created_at,
            parsed_repost,
            parsed_wordle,
            parsed_embed,
            deleted,
            checked_old,
        }
    }

    /// Returns a URI that references the message in discord. When clicked inside a
    /// discord client it will auto scroll to the message
    pub fn uri(&self) -> String {
        MessageId(self.id).link(ChannelId(self.channel), Some(GuildId(self.server)))
    }

    pub const fn is_repost_parsed(&self) -> bool {
        self.parsed_repost.is_some()
    }

    pub const fn is_wordle_parsed(&self) -> bool {
        self.parsed_wordle.is_some()
    }

    pub const fn is_embed_parsed(&self) -> bool {
        self.parsed_embed.is_some()
    }

    pub const fn is_deleted(&self) -> bool {
        self.deleted.is_some()
    }

    /// Return true if the link has been marked as 'checked for old messages'.
    /// After querying for old messages around a given message OR the message
    /// appears in an old message query (i.e. it was already in the db but
    /// showed up in an old query anyways) it should be marked as checked.
    ///
    /// When a new field is added that requires going and back checking, all
    /// messages should return false. If possible to determine that not all
    /// messages need to be checked, only the messages that need to be checked
    /// should start returning false to reduce backlog.
    pub const fn is_checked_old(&self) -> bool {
        self.checked_old.is_some()
    }

    pub fn get_duration(&self, current: DateTime<Utc>) -> Option<Duration> {
        match current.signed_duration_since(self.created_at).to_std() {
            Ok(ret) => Some(ret),
            Err(err) => {
                debug!(
                    "failed to calculate duration from object {} to input {current} with err {err:?}",
                    self.created_at
                );
                None
            }
        }
    }
}
