use super::Handler;

use serenity::{model::channel::ChannelType, model::channel::Message, prelude::*};
use std::collections::HashMap;

impl Handler {
    pub async fn handle_command(&self, ctx: &Context, msg: &Message) {
        if !msg.content.starts_with("!rpm") || !msg.content.len() <= 4 {
            return;
        }

        let command = &msg.content[4..].trim();
        match command {
            &"pins" => self.pins(ctx, msg).await,
            _ => println!("Received unknown command: \"{}\"", command),
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
            Err(why) => println!("Failed to inform of REPOST: {:?}", why),
        }
    }
}
