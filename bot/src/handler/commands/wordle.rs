use crate::errors::Result;
use crate::structs::reply::{Reply, ReplyType};

use db::structs::wordle::Wordle;
use db::DB;
use serenity::{model::channel::Message, prelude::*};

fn wordle_score_distribution(wordles: &[Wordle]) -> String {
    fn parse_row(total: usize, count: usize, i: usize) -> String {
        let percent = (count as f32) / (total as f32) * 100.0;
        format!(
            "{}: {} {}%",
            match i {
                0 => "X".to_string(),
                _ => i.to_string(),
            },
            "🟩".repeat((percent / 4.0) as usize),
            percent as usize
        )
    }

    let len = wordles.len();
    let scores = wordles
        .iter()
        .fold([0, 0, 0, 0, 0, 0, 0], |mut acc, wordle| {
            acc[wordle.score as usize] += 1;
            acc
        });
    let rows = scores[1..]
        .iter()
        .enumerate()
        .map(|(i, count)| parse_row(len, *count, i + 1))
        .collect::<Vec<String>>();
    let mut ret = String::new();
    for r in rows {
        ret.push_str(&format!("{}\n", r));
    }
    ret.push_str(&parse_row(len, scores[0], 0));

    ret
}

pub async fn wordle_score_user<'a>(ctx: &Context, msg: &'a Message) -> Result<Reply<'a>> {
    let wordles = DB::db_call(|db| {
        db.get_wordles_for_author(*msg.author.id.as_u64(), *msg.guild_id.unwrap().as_u64())
    })?;

    let name = match msg.guild_id {
        Some(guild_id) => msg.author.nick_in(&ctx, guild_id).await,
        None => None,
    };

    let resp = format!(
        "Wordle distribution for {}\n{}",
        name.unwrap_or_else(|| msg.author.name.clone()),
        wordle_score_distribution(&wordles)
    );

    Ok(Reply::new(resp, ReplyType::Message(msg)))
}

pub fn wordle_score_server(msg: &Message) -> Result<Reply<'_>> {
    let wordles = DB::db_call(|db| db.get_wordles_for_server(*msg.guild_id.unwrap().as_u64()))?;

    Ok(Reply::new(
        format!(
            "Wordle distribution for server\n{}",
            wordle_score_distribution(&wordles)
        ),
        ReplyType::Message(msg),
    ))
}
