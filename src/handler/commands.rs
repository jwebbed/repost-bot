use super::Handler;

use crate::db::DB;
use crate::structs::wordle::{Wordle};
use serenity::{model::channel::ChannelType, model::channel::Message, prelude::*};
use std::collections::HashMap;

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
    pub async fn handle_command(&self, ctx: &Context, msg: &Message) {
        if !msg.content.starts_with("!rpm") || !msg.content.len() <= 4 {
            return;
        }

        let command = &msg.content[4..].trim();
        match *command {
            "pins" => self.pins(ctx, msg).await,
            "reposts" => self.repost_cnt(ctx, msg).await,
            "wordle score" => self.wordle_score(ctx, msg).await,
            _ => println!("Received unknown command: \"{}\"", command),
        }
    }

    async fn wordle_score(&self, ctx: &Context, msg: &Message) {
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

    async fn pins(&self, ctx: &Context, msg: &Message) {
        let guild = msg.guild_id.unwrap();

        let channels = match guild.channels(&ctx.http).await {
            Ok(x) => x,
            Err(_) => HashMap::new(),
        };
        let mut pins = Vec::<Message>::new();
        for (_, channel) in channels.iter() {
            if channel.kind == ChannelType::Text {
                pins.extend(match channel.pins(&ctx.http).await {
                    Ok(x) => x,
                    Err(_) => Vec::new(),
                });
            }
        }

        let mut pin_cnt: HashMap<String, usize> = HashMap::new();
        for pin in pins {
            let user = pin.author.name;
            let new_cnt = if pin_cnt.contains_key(&user) {
                pin_cnt[&user] + 1
            } else {
                1
            };
            pin_cnt.insert(user, new_cnt);
        }

        let mut tuples = Vec::new();
        for (user, cnt) in pin_cnt.iter() {
            tuples.push((user, cnt));
        }

        tuples.sort_by_key(|x| x.1);
        tuples.reverse();
        println!("{:?}", tuples);

        let response = format!(
            "the chamPIoNship\n{}",
            tuples
                .into_iter()
                .map(|x| format!("{}: with {} pins", x.0, x.1))
                .collect::<Vec<String>>()
                .join("\n")
        );

        match msg.channel_id.say(&ctx.http, response).await {
            Ok(_) => (),
            Err(why) => println!("Failed to inform of PINS: {:?}", why),
        }
    }

    async fn repost_cnt(&self, ctx: &Context, msg: &Message) {
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

        match msg.channel_id.say(&ctx.http, response).await {
            Ok(_) => (),
            Err(why) => println!("Failed to inform of REPOST COUNT: {:?}", why),
        }
    }
}
