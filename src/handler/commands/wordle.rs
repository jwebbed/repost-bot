use super::Handler;

use crate::db::DB;
use crate::structs::wordle::Wordle;
use serenity::{model::channel::Message, prelude::*};

fn wordle_score_distribution(wordles: &Vec<Wordle>) -> String {
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

impl Handler {
    pub async fn wordle_score(&self, ctx: &Context, msg: &Message) {
        let query = DB::db_call(|db| {
            db.get_wordles_for_author(*msg.author.id.as_u64(), *msg.guild_id.unwrap().as_u64())
        });

        if let Ok(wordles) = query {
            match msg
                .reply(&ctx.http, wordle_score_distribution(&wordles))
                .await
            {
                Ok(_) => (),
                Err(why) => println!("Failed to inform of wordle distribution: {:?}", why),
            }
        }
    }
}
