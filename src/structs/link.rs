use chrono::{DateTime, Utc};
use serenity::model::id::{ChannelId, GuildId, MessageId};

#[derive(Debug)]
pub struct Message {
    // snowflakes referencing the various attributes
    pub id: u64,     // the snowflake of this message
    pub server: u64, // called guilds for some reason in discord API
    pub channel: u64,
    pub author: Option<u64>,

    // time that the message for this link was created
    pub created_at: DateTime<Utc>,

    // flags to indicate if various things were processed
    pub parsed_repost: Option<DateTime<Utc>>,
    pub parsed_wordle: Option<DateTime<Utc>>,
    pub deleted: Option<DateTime<Utc>>,
    pub checked_old: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct Channel {
    pub id: u64,
    pub name: Option<String>,
    pub visible: bool,
    pub server: u64,
}

#[derive(Debug)]
pub struct Link {
    pub id: Option<usize>, // internal to us
    pub link: String,

    pub message: Message,

    // not actually stored in table, just here for convience
    pub channel_name: Option<String>,
    pub server_name: Option<String>,
}

impl Message {
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

    pub const fn is_deleted(&self) -> bool {
        self.deleted.is_some()
    }

    pub const fn is_checked_old(&self) -> bool {
        self.checked_old.is_some()
    }
}
