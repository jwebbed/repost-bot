mod pins;
mod wordle;

use super::Handler;

use crate::db::DB;
use crate::errors::Result;
use crate::structs::reply::{Reply, ReplyType};
use serenity::{model::channel::Message, prelude::*};

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

impl Handler {
    pub async fn handle_command<'a>(
        &'a self,
        ctx: &Context,
        msg: &'a Message,
    ) -> Option<Reply<'a>> {
        if !msg.content.starts_with("!rpm") || !msg.content.len() <= 4 {
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
                println!("Failed to process command {command} with err: {why}");
                None
            }
        }
    }
}
