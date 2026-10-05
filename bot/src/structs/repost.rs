use crate::structs::reply::{Reply, ReplyType};

use chrono::{DateTime, Utc};
use db::structs::Message;
use itertools::Itertools;
use log::info;
use serenity::model;
use serenity::model::id::MessageId;
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum RepostType {
    Link,
    Image,
}

#[derive(Debug, Default)]
pub struct RepostSet {
    reposts: BTreeMap<Message, HashSet<RepostType>>,
    types: HashSet<RepostType>,
}

impl RepostSet {
    pub fn new_from_messages(messages: &[Message], repost_type: RepostType) -> RepostSet {
        let mut set = RepostSet::default();
        for msg in messages {
            set.add(*msg, repost_type);
        }
        set
    }

    pub fn add(&mut self, msg: Message, repost_type: RepostType) {
        self.reposts.entry(msg).or_default().insert(repost_type);
        self.types.insert(repost_type);
    }

    pub fn union(&mut self, other: &RepostSet) {
        for (msg, repost_types) in &other.reposts {
            self.reposts
                .entry(*msg)
                .or_default()
                .extend(repost_types.iter().copied());
        }
        self.types.extend(other.types.iter().copied());
    }

    pub fn len(&self) -> usize {
        self.reposts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.reposts.is_empty()
    }

    pub fn generate_reply_for_message_id(
        &self,
        msg_id: model::id::MessageId,
        channel_id: model::id::ChannelId,
        msg_created_at: DateTime<Utc>,
    ) -> Option<Reply> {
        self.generate_reply(msg_created_at)
            .map(|x| Reply::new(x, ReplyType::Message(msg_id, channel_id)))
    }

    pub fn generate_reply_for_message(
        &self,
        msg: &serenity::model::prelude::Message,
    ) -> Option<Reply> {
        self.generate_reply(*msg.id.created_at())
            .map(|x| Reply::new(x, ReplyType::from(msg)))
    }

    fn generate_reply(&self, reply_to_created_at: DateTime<Utc>) -> Option<String> {
        if !self.reposts.is_empty() {
            info!("generating reply for {self:?}");
        }
        match self.reposts.len() {
            0 => None,
            1 => {
                let (msg, rtypes) = self.reposts.iter().next()?;
                let prefix = prefix_text(rtypes, true);
                let link_text = repost_text(msg, reply_to_created_at);
                Some(format!("🚨 {prefix} 🚨 REPOST 🚨 {link_text}"))
            }
            _ => {
                let lines = self
                    .reposts
                    .iter()
                    .map(|(repost_msg, repost_types)| {
                        let text = repost_text(repost_msg, reply_to_created_at);
                        if self.types.len() > 1 {
                            format!("{} {text}", prefix_text(repost_types, false))
                        } else {
                            text
                        }
                    })
                    .join("\n");

                let header_prefix = prefix_text(&self.types, true);
                Some(format!("🚨 {header_prefix} 🚨 REPOST 🚨\n{lines}"))
            }
        }
    }
}

impl RepostType {
    const fn text_long(self) -> &'static str {
        match self {
            RepostType::Link => "LINK",
            RepostType::Image => "IMAGE",
        }
    }

    const fn text_short(self) -> &'static str {
        match self {
            RepostType::Link => "🔗",
            RepostType::Image => "🖼️",
        }
    }
}

fn prefix_text(repost_types: &HashSet<RepostType>, long_text: bool) -> String {
    let (text, separator): (fn(RepostType) -> &'static str, _) = if long_text {
        (RepostType::text_long, "/")
    } else {
        (RepostType::text_short, "")
    };
    repost_types
        .iter()
        .map(|t| text(*t))
        .sorted_unstable()
        .join(separator)
}

fn repost_text(original_message: &Message, reply_to_created_at: DateTime<Utc>) -> String {
    let uri = build_uri(original_message);
    match original_message.get_duration(reply_to_created_at) {
        Some(duration) => format!("{} {uri}", format_age(duration)),
        None => uri,
    }
}

/// Units used to describe how old a message is, largest first
const AGE_UNITS: [(u64, &str); 6] = [
    (365 * 24 * 60 * 60, "y"),
    (30 * 24 * 60 * 60, "mo"),
    (24 * 60 * 60, "d"),
    (60 * 60, "h"),
    (60, "m"),
    (1, "s"),
];

/// Formats a duration using its largest unit and the unit below it, e.g.
/// "1d 14h" or "22h" rather than every unit down to milliseconds
fn format_age(duration: Duration) -> String {
    let mut remaining = duration.as_secs();
    let Some(largest) = AGE_UNITS.iter().position(|(secs, _)| remaining >= *secs) else {
        return "0s".to_string();
    };
    let mut age = String::new();
    for (secs, unit) in AGE_UNITS.iter().skip(largest).take(2) {
        let count = remaining / secs;
        remaining %= secs;
        if count > 0 {
            if !age.is_empty() {
                age.push(' ');
            }
            age.push_str(&format!("{count}{unit}"));
        }
    }
    age
}

#[inline]
fn build_uri(message: &Message) -> String {
    MessageId::new(message.id).link(message.channel.into(), Some(message.server.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::prelude::*;

    const fn get_message(id: u64, server: u64, channel: u64, created_at: DateTime<Utc>) -> Message {
        Message::new(
            id, server, channel, None, created_at, None, None, None, None,
        )
    }

    fn get_datetime(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2022, 5, 1, h, m, s).unwrap()
    }

    #[test]
    fn test_set_union_overlap() {
        let msg = get_message(1, 1, 1, get_datetime(1, 0, 0));
        let mut set1 = RepostSet::default();
        let mut set2 = RepostSet::default();
        set1.add(msg, RepostType::Image);
        set2.add(msg, RepostType::Image);

        set1.union(&set2);
        assert_eq!(set1.len(), 1);
    }

    #[test]
    fn test_set_union_no_overlap() {
        let mut set1 = RepostSet::default();
        let mut set2 = RepostSet::default();
        set1.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );
        set2.add(
            get_message(2, 1, 1, get_datetime(1, 0, 1)),
            RepostType::Link,
        );

        set1.union(&set2);
        assert_eq!(set1.len(), 2);
    }

    #[test]
    fn test_single_image_repost() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );

        let reply_str = set.generate_reply(get_datetime(2, 0, 0));
        assert_eq!(
            Some("🚨 IMAGE 🚨 REPOST 🚨 1h https://discord.com/channels/1/1/1".to_string()),
            reply_str
        );
    }

    #[test]
    fn test_single_link_repost() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Link,
        );

        let reply_str = set.generate_reply(get_datetime(2, 0, 0));
        assert_eq!(
            Some("🚨 LINK 🚨 REPOST 🚨 1h https://discord.com/channels/1/1/1".to_string()),
            reply_str
        );
    }

    #[test]
    fn test_multi_image_repost() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Image,
        );

        let reply_str = set.generate_reply(Utc.with_ymd_and_hms(2022, 5, 1, 3, 0, 0).unwrap());
        assert_eq!(
            Some(
                "🚨 IMAGE 🚨 REPOST 🚨\n\
            2h https://discord.com/channels/1/1/1\n\
            1h https://discord.com/channels/1/1/2"
                    .to_string()
            ),
            reply_str
        );
    }

    #[test]
    fn test_multi_link_repost() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Link,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Link,
        );

        let reply_str = set.generate_reply(Utc.with_ymd_and_hms(2022, 5, 1, 3, 0, 0).unwrap());
        assert_eq!(
            Some(
                "🚨 LINK 🚨 REPOST 🚨\n\
            2h https://discord.com/channels/1/1/1\n\
            1h https://discord.com/channels/1/1/2"
                    .to_string()
            ),
            reply_str
        );
    }

    #[test]
    fn test_multi_image_link_reposts_seperate() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Link,
        );

        let reply_str = set.generate_reply(Utc.with_ymd_and_hms(2022, 5, 1, 3, 0, 0).unwrap());
        assert_eq!(
            Some(
                "🚨 IMAGE/LINK 🚨 REPOST 🚨\n\
            🖼️ 2h https://discord.com/channels/1/1/1\n\
            🔗 1h https://discord.com/channels/1/1/2"
                    .to_string()
            ),
            reply_str
        );
    }

    #[test]
    fn test_multi_image_link_reposts_overlap() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Link,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Image,
        );

        let reply_str = set.generate_reply(Utc.with_ymd_and_hms(2022, 5, 1, 3, 0, 0).unwrap());
        assert_eq!(
            Some(
                "🚨 IMAGE/LINK 🚨 REPOST 🚨\n\
            🖼️ 2h https://discord.com/channels/1/1/1\n\
            🔗🖼️ 1h https://discord.com/channels/1/1/2"
                    .to_string()
            ),
            reply_str
        );
    }

    #[test]
    fn test_single_repost_image_link() {
        let mut set = RepostSet::default();
        let msg = get_message(1, 1, 1, get_datetime(1, 0, 0));
        set.add(msg, RepostType::Link);
        set.add(msg, RepostType::Image);

        let reply_str = set.generate_reply(Utc.with_ymd_and_hms(2022, 5, 1, 2, 0, 0).unwrap());
        assert_eq!(
            Some("🚨 IMAGE/LINK 🚨 REPOST 🚨 1h https://discord.com/channels/1/1/1".to_string()),
            reply_str
        );
    }

    #[test]
    fn test_empty_set_has_no_reply() {
        let set = RepostSet::default();
        assert!(set.is_empty());
        assert_eq!(set.generate_reply(get_datetime(1, 0, 0)), None);
    }

    #[test]
    fn test_new_from_messages() {
        let set = RepostSet::new_from_messages(
            &[
                get_message(1, 1, 1, get_datetime(1, 0, 0)),
                get_message(2, 1, 1, get_datetime(1, 30, 0)),
            ],
            RepostType::Link,
        );
        assert_eq!(set.len(), 2);
        assert_eq!(
            Some(
                "🚨 LINK 🚨 REPOST 🚨\n\
            1h https://discord.com/channels/1/1/1\n\
            30m https://discord.com/channels/1/1/2"
                    .to_string()
            ),
            set.generate_reply(get_datetime(2, 0, 0))
        );
    }

    #[test]
    fn test_union_merges_types() {
        let msg = get_message(1, 1, 1, get_datetime(1, 0, 0));
        let mut links = RepostSet::new_from_messages(&[msg], RepostType::Link);
        let images = RepostSet::new_from_messages(&[msg], RepostType::Image);
        links.union(&images);
        assert_eq!(links.len(), 1);
        assert_eq!(
            Some("🚨 IMAGE/LINK 🚨 REPOST 🚨 1h https://discord.com/channels/1/1/1".to_string()),
            links.generate_reply(get_datetime(2, 0, 0))
        );
    }

    #[test]
    fn test_reply_omits_duration_when_original_is_newer() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Link,
        );
        assert_eq!(
            Some("🚨 LINK 🚨 REPOST 🚨 https://discord.com/channels/1/1/1".to_string()),
            set.generate_reply(get_datetime(1, 0, 0))
        );
    }

    #[test]
    fn test_generate_reply_for_message_id() {
        let mut set = RepostSet::default();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Link,
        );
        let msg_id = model::id::MessageId::new(10);
        let channel_id = model::id::ChannelId::new(20);
        let reply = set
            .generate_reply_for_message_id(msg_id, channel_id, get_datetime(2, 0, 0))
            .unwrap();
        assert_eq!(
            reply,
            Reply::new(
                "🚨 LINK 🚨 REPOST 🚨 1h https://discord.com/channels/1/1/1",
                ReplyType::Message(msg_id, channel_id)
            )
        );
        assert!(
            RepostSet::default()
                .generate_reply_for_message_id(msg_id, channel_id, get_datetime(2, 0, 0))
                .is_none()
        );
    }

    #[test]
    fn test_format_age() {
        let secs = Duration::from_secs;
        let (minute, hour, day) = (60, 60 * 60, 24 * 60 * 60);
        assert_eq!(format_age(Duration::from_millis(359)), "0s");
        assert_eq!(format_age(secs(45)), "45s");
        assert_eq!(format_age(secs(5 * minute + 3)), "5m 3s");
        assert_eq!(format_age(secs(hour)), "1h");
        assert_eq!(format_age(secs(22 * hour + 52 * minute + 3)), "22h 52m");
        // the screenshots from discord
        assert_eq!(
            format_age(secs(day + 14 * hour + 24 * minute + 54) + Duration::from_millis(145)),
            "1d 14h"
        );
        assert_eq!(
            format_age(secs(61 * day + 18 * hour + 54 * minute)),
            "2mo 1d"
        );
        // only adjacent units are shown
        assert_eq!(format_age(secs(day + 5 * minute)), "1d");
        assert_eq!(format_age(secs(400 * day)), "1y 1mo");
    }
}
