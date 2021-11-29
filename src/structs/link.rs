use chrono::{DateTime, Duration, Utc};
use serenity::model::channel::Message;
use serenity::model::id::{ChannelId, GuildId, MessageId};

#[derive(Debug)]
pub struct Link {
    pub id: Option<usize>, // internal to us
    pub link: String,

    // snowflakes referencing the various attributes
    pub server: u64, // called guilds for some reason in discord API
    pub channel: u64,
    pub message: u64,

    // time that the message for this link was created
    pub created_at: DateTime<Utc>,

    // not actually stored in table, just here for convience
    pub channel_name: Option<String>,
    pub server_name: Option<String>,
}

impl Link {
    /// Returns a URI that references the message in discord. When clicked inside a
    /// discord client it will auto scroll to the message
    pub fn message_uri(&self) -> String {
        MessageId(self.message).link(ChannelId(self.channel), Some(GuildId(self.server)))
    }
}
