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

        println!("pins: {:?}", pins);
    }
}
