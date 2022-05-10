use crate::structs::reply::{Reply, ReplyType};
use crate::structs::Message;

use humantime::format_duration;
use log::error;
use std::collections::HashSet;
use std::vec::Vec;

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
pub enum RepostType {
    Link,
    Image,
}

#[derive(Hash, Eq, PartialEq, Debug, Copy, Clone)]
struct Repost {
    repost_type: RepostType,
    message: Message,
}

pub struct RepostSet {
    reposts: HashSet<Repost>,
}

#[allow(dead_code, unused_must_use)]
impl RepostSet {
    pub fn new() -> RepostSet {
        RepostSet {
            reposts: HashSet::new(),
        }
    }

    pub fn add(&mut self, repost_type: RepostType, message: Message) {
        self.reposts.insert(Repost {
            repost_type,
            message,
        });
    }

    pub fn union(&mut self, other: &RepostSet) {
        self.reposts.union(&other.reposts);
    }

    pub fn len(&self) -> usize {
        self.reposts.len()
    }

    pub fn generate_reply_for_message<'a>(
        &self,
        msg: &'a serenity::model::prelude::Message,
    ) -> Option<Reply<'a>> {
        if !self.reposts.is_empty() {
            let response = if self.reposts.len() > 1 {
                // need to do something more advanced here to account for multiple types of reposts
                let mut to_process = Vec::from_iter(self.reposts.clone());
                to_process.sort_by(|a, b| a.message.created_at.cmp(&b.message.created_at));

                Some(format!(
                    "🚨 REPOST 🚨\n{}",
                    to_process
                        .iter()
                        .map(|x| repost_text(&x.message, msg))
                        .collect::<Vec<String>>()
                        .join("\n")
                ))
            } else {
                if let Some(repost) = self.reposts.iter().next() {
                    let prefix = match repost.repost_type {
                        RepostType::Link => "LINK",
                        RepostType::Image => "IMAGE",
                    };
                    let link_text = repost_text(&repost.message, msg);

                    Some(format!("🚨 {prefix} 🚨 REPOST 🚨 {link_text}"))
                } else {
                    // in principle this code path should be impossible since we've already checked the length
                    error!(
                        "RepostSet had 1 element but got None when extracting it {:?}",
                        self.reposts
                    );
                    None
                }
            };

            response.map(|x| Reply::new(x, ReplyType::Message(msg)))
        } else {
            None
        }
    }
}

fn repost_text(
    repost_message: &Message,
    response_msg: &serenity::model::prelude::Message,
) -> String {
    /*  let duration_text = match {
        Some(duration) => format_duration(duration).to_string(),
        None => "".to_string(),
    };*/

    format!(
        "{} {}",
        repost_message
            .get_duration(*response_msg.id.created_at())
            .map_or("".to_string(), |duration| format_duration(duration)
                .to_string()),
        repost_message.uri()
    )
}
