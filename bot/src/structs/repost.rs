use crate::structs::reply::{Reply, ReplyType};

use chrono::{DateTime, Utc};
use db::structs::Message;
use humantime::format_duration;
use log::info;
use serenity::model;
use std::collections::{HashMap, HashSet};
use std::vec::Vec;

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum RepostType {
    Link,
    Image,
}

#[derive(Debug)]
pub struct RepostSet {
    reposts: HashMap<Message, HashSet<RepostType>>,
    types: HashSet<RepostType>,
}

impl RepostSet {
    pub fn new() -> RepostSet {
        RepostSet {
            reposts: HashMap::new(),
            types: HashSet::new(),
        }
    }

    pub fn new_from_messages(messages: &[Message], repost_type: RepostType) -> RepostSet {
        RepostSet {
            reposts: messages
                .iter()
                .map(|m| (*m, HashSet::from([repost_type])))
                .collect(),
            types: HashSet::from([repost_type]),
        }
    }

    pub fn add(&mut self, msg: Message, repost_type: RepostType) {
        self.reposts
            .entry(msg)
            .or_insert_with(HashSet::new)
            .insert(repost_type);
        self.types.insert(repost_type);
    }

    pub fn union(&mut self, other: &RepostSet) {
        // Should clean this up, can probably do it with some clever maps
        for (msg, repost_types) in &other.reposts {
            for repost_type in repost_types {
                self.add(*msg, *repost_type);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.reposts.len()
    }

    pub fn generate_reply_for_message_id<'a>(
        &self,
        msg_id: &'a model::id::MessageId,
        channel_id: &'a model::id::ChannelId,
        msg_created_at: DateTime<Utc>,
    ) -> Option<Reply<'a>> {
        self.generate_reply(msg_created_at)
            .map(|x| Reply::new(x, ReplyType::MessageId(*msg_id, *channel_id)))
    }

    pub fn generate_reply_for_message<'a>(
        &self,
        msg: &'a serenity::model::prelude::Message,
    ) -> Option<Reply<'a>> {
        self.generate_reply(*msg.id.created_at())
            .map(|x| Reply::new(x, ReplyType::Message(msg)))
    }

    fn generate_reply(&self, reply_to_created_at: DateTime<Utc>) -> Option<String> {
        if !self.reposts.is_empty() {
            info!("generating reply for {self:?}");
        }
        match self.reposts.len() {
            0 => None,
            1 => {
                let (msg, rtypes) = self.reposts.iter().next().unwrap();
                let prefix = prefix_text(rtypes, true);
                let link_text = repost_text(msg, reply_to_created_at);
                Some(format!("🚨 {prefix} 🚨 REPOST 🚨 {link_text}"))
            }
            _ => {
                let mut msgs_mut: Vec<Message> = self.reposts.clone().into_keys().collect();
                msgs_mut.sort_by(|a, b| a.created_at.cmp(&b.created_at));
                let msgs = msgs_mut;

                let lines = msgs
                    .iter()
                    .map(|x| {
                        let text = repost_text(x, reply_to_created_at);
                        if self.types.len() > 1 {
                            // should never panic since this is literally just an iter of the sorted keys
                            let thing = self.reposts.get(x).unwrap();
                            format!("{} {text}", prefix_text(thing, false))
                        } else {
                            text
                        }
                    })
                    .collect::<Vec<String>>()
                    .join("\n");

                let header_prefix = format!("{} 🚨 ", prefix_text(&self.types, true));
                Some(format!("🚨 {}REPOST 🚨\n{}", header_prefix, lines))
            }
        }
    }
}

impl RepostType {
    const fn text_long(&self) -> &str {
        match self {
            RepostType::Link => "LINK",
            RepostType::Image => "IMAGE",
        }
    }

    const fn text_short(&self) -> &str {
        match self {
            RepostType::Link => "🔗",
            RepostType::Image => "🖼️",
        }
    }
}

fn prefix_text(repost_types: &HashSet<RepostType>, long_text: bool) -> String {
    let mut labels: Vec<&str> = repost_types
        .iter()
        .map(|t| {
            if long_text {
                t.text_long()
            } else {
                t.text_short()
            }
        })
        .collect();
    labels.sort_unstable();
    if long_text {
        labels.join("/")
    } else {
        labels.join("")
    }
}

fn repost_text(original_message: &Message, reply_to_created_at: DateTime<Utc>) -> String {
    format!(
        "{} {}",
        original_message
            .get_duration(reply_to_created_at)
            .map_or("".to_string(), |duration| format_duration(duration)
                .to_string()),
        original_message.uri()
    )
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
        Utc.ymd(2022, 5, 1).and_hms(h, m, s)
    }

    #[test]
    fn test_set_union_overlap() {
        let msg = get_message(1, 1, 1, get_datetime(1, 0, 0));
        let mut set1 = RepostSet::new();
        let mut set2 = RepostSet::new();
        set1.add(msg, RepostType::Image);
        set2.add(msg, RepostType::Image);

        set1.union(&set2);
        assert_eq!(set1.len(), 1);
    }

    #[test]
    fn test_set_union_no_overlap() {
        let mut set1 = RepostSet::new();
        let mut set2 = RepostSet::new();
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
        let mut set = RepostSet::new();
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
        let mut set = RepostSet::new();
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
        let mut set = RepostSet::new();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Image,
        );

        let reply_str = set.generate_reply(Utc.ymd(2022, 5, 1).and_hms(3, 0, 0));
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
        let mut set = RepostSet::new();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Link,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Link,
        );

        let reply_str = set.generate_reply(Utc.ymd(2022, 5, 1).and_hms(3, 0, 0));
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
        let mut set = RepostSet::new();
        set.add(
            get_message(1, 1, 1, get_datetime(1, 0, 0)),
            RepostType::Image,
        );

        set.add(
            get_message(2, 1, 1, get_datetime(2, 0, 0)),
            RepostType::Link,
        );

        let reply_str = set.generate_reply(Utc.ymd(2022, 5, 1).and_hms(3, 0, 0));
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
        let mut set = RepostSet::new();
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

        let reply_str = set.generate_reply(Utc.ymd(2022, 5, 1).and_hms(3, 0, 0));
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
        let mut set = RepostSet::new();
        let msg = get_message(1, 1, 1, get_datetime(1, 0, 0));
        set.add(msg, RepostType::Link);
        set.add(msg, RepostType::Image);

        let reply_str = set.generate_reply(Utc.ymd(2022, 5, 1).and_hms(2, 0, 0));
        assert_eq!(
            Some("🚨 IMAGE/LINK 🚨 REPOST 🚨 1h https://discord.com/channels/1/1/1".to_string()),
            reply_str
        );
    }
}
