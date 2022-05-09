use crate::structs::reply::{Reply, ReplyType};
use crate::structs::Message;

use humantime::format_duration;
use std::cell::RefCell;
use std::time::{Duration, Instant};
use std::vec::Vec;

#[derive(Debug, Copy, Clone)]
pub enum RepostType {
    Link,
    Image,
}

#[derive(Debug, Copy, Clone)]
struct Repost {
    repost_type: RepostType,
    message: Message,
}

pub struct RepostSet {
    reposts: RefCell<Vec<Repost>>,
}

impl RepostSet {
    pub fn new() -> RepostSet {
        RepostSet {
            reposts: RefCell::new(vec![]),
        }
    }

    pub fn add(&self, repost_type: RepostType, message: Message) {
        self.reposts.borrow_mut().push(Repost {
            repost_type,
            message,
        });
    }

    pub fn union(&self, other: RepostSet) {
        self.reposts
            .borrow_mut()
            .extend(other.reposts.borrow().iter());
    }

    pub fn len(&self) -> usize {
        self.reposts.borrow().len()
    }

    pub fn generate_reply_for_message<'a>(
        &self,
        msg: &'a serenity::model::prelude::Message,
    ) -> Option<Reply<'a>> {
        let reposts = self.reposts.borrow();
        if !reposts.is_empty() {
            let repost_str = if reposts.len() > 1 {
                format!(
                    "\n{}",
                    reposts
                        .iter()
                        .map(|x| repost_text(x, msg))
                        .collect::<Vec<String>>()
                        .join("\n")
                )
            } else {
                repost_text(&reposts[0], msg)
            };
            Some(Reply::new(
                format!("🚨 REPOST 🚨 {repost_str}"),
                ReplyType::Message(msg),
            ))
        } else {
            None
        }
    }
}

fn repost_text(repost: &Repost, msg: &serenity::model::prelude::Message) -> String {
    let duration_text = match repost.message.get_duration(*msg.id.created_at()) {
        Some(duration) => format_duration(duration).to_string(),
        None => "".to_string(),
    };

    format!("{} {}", duration_text, repost.message.uri())
}

/*
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
*/
