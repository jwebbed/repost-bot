use crate::structs::reply::{Reply, ReplyType};
use crate::structs::Message;

use chrono::{DateTime, Utc};
use humantime::format_duration;
use log::{error, warn};
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
        warn!("generating reply for {self:?}");
        if self.reposts.len() > 1 {
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
        } else if self.reposts.len() == 1 {
            self.reposts.iter().next().map_or_else(
                || {
                    // in principle this code path should be impossible since we've already checked the length
                    error!(
                        "RepostSet had 1 element but got None when extracting it {:?}",
                        self.reposts
                    );
                    None
                },
                |(msg, rtypes)| {
                    let prefix = prefix_text(rtypes, true);
                    let link_text = repost_text(msg, reply_to_created_at);
                    Some(format!("🚨 {prefix} 🚨 REPOST 🚨 {link_text}"))
                },
            )
        } else {
            None
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
