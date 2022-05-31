mod pins;
mod wordle;

use crate::errors::Result;
use crate::structs::reply::{Reply, ReplyType};
use db::DB;

use lazy_static::lazy_static;
use log::warn;
use regex::Regex;
use serenity::{model::channel::Message, prelude::*};

pub(super) fn has_command_prefix(command: &str) -> bool {
    lazy_static! {
        static ref RE: Regex = Regex::new(r"(?i)^!rp(m|b) ").unwrap();
    }
    RE.is_match(command)
}

fn repost_cnt(msg: &Message) -> Result<Reply<'_>> {
    let reposts = match DB::db_call(|db| db.get_repost_list(*msg.guild_id.unwrap().as_u64())) {
        Ok(r) => r,
        Err(_) => Vec::new(),
    };

    let response = format!(
        "Count | Link\n{}",
        reposts
            .into_iter()
            .map(|x| format!("{:<9} | <{}>", x.count, x.link))
            .collect::<Vec<String>>()
            .join("\n")
    );

    Ok(Reply::new(response, ReplyType::Channel(msg.channel_id)))
}

fn reposter_cnt(msg: &Message) -> Result<Reply<'_>> {
    let reposters = match DB::db_call(|db| db.get_top_reposters(*msg.guild_id.unwrap().as_u64())) {
        Ok(r) => r,
        Err(_) => Vec::new(),
    };

    let response = format!(
        "Username | Count\n{}",
        reposters
            .into_iter()
            .map(|x| format!("{} | {:<9}", x.username, x.count))
            .collect::<Vec<String>>()
            .join("\n")
    );

    Ok(Reply::new(response, ReplyType::Channel(msg.channel_id)))
}

pub async fn handle_command<'a>(ctx: &Context, msg: &'a Message) -> Option<Reply<'a>> {
    // checking command prefix twice current, should refactor to not do that
    if !msg.content.len() <= 4 || !has_command_prefix(&msg.content) {
        return None;
    }

    let command = &msg.content[4..].trim();
    let ret = match *command {
        "pins" => pins::pins(ctx, msg).await,
        "reposts" => repost_cnt(msg),
        "reposters" => reposter_cnt(msg),
        "wordle score" => wordle::wordle_score_user(ctx, msg).await,
        "wordle server" => wordle::wordle_score_server(msg),
        _ => Ok(Reply::new_const(
            "Unrecognized command",
            ReplyType::Message(msg),
        )),
    };

    match ret {
        Ok(resp) => Some(resp),
        Err(why) => {
            warn!("Failed to process command {command} with err: {why}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_command_prefix_rpm() {
        assert!(has_command_prefix("!rpm pins"));
        assert!(has_command_prefix("!rpm reposts"));
        assert!(has_command_prefix("!rpm reposters"));
        assert!(has_command_prefix("!rpm wordle score"));
        assert!(has_command_prefix("!rpm wordle server"));
        assert!(has_command_prefix("!rpm allowlist"));
    }

    #[test]
    fn test_command_prefix_rpb() {
        assert!(has_command_prefix("!rpb pins"));
        assert!(has_command_prefix("!rpb reposts"));
        assert!(has_command_prefix("!rpb reposters"));
        assert!(has_command_prefix("!rpb wordle score"));
        assert!(has_command_prefix("!rpb wordle server"));
        assert!(has_command_prefix("!rpb allowlist"));
    }

    #[test]
    fn test_command_prefix_not_start() {
        assert!(!has_command_prefix("   !rpb pins"));
    }

    #[test]
    fn test_command_prefix_no_exclaimation() {
        assert!(!has_command_prefix("rpb pins"));
    }

    #[test]
    fn test_command_prefix_non_command() {
        assert!(!has_command_prefix(""));
        assert!(!has_command_prefix("!"));
        assert!(!has_command_prefix("hello world!"));
    }
}
