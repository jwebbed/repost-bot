use crate::errors::Result;
use crate::handler::bot_read_channel_permission;
use crate::structs::reply::{Reply, ReplyType};

use log::trace;
use serenity::{model::channel::ChannelType, model::channel::Message, prelude::*};
use std::collections::HashMap;

pub async fn pins<'a>(ctx: &Context, msg: &'a Message) -> Result<Reply<'a>> {
    let guild = msg.guild_id.unwrap();

    let channels = guild.channels(&ctx.http).await?;
    let mut pins = Vec::<Message>::new();
    for (_, channel) in channels.iter() {
        let visible = bot_read_channel_permission(&ctx, channel).await;
        if visible && channel.kind == ChannelType::Text {
            pins.extend(channel.pins(&ctx.http).await?);
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
    trace!("found the following pins {tuples:?}");

    let response = format!(
        "the chamPIoNship\n{}",
        tuples
            .into_iter()
            .map(|x| format!("{}: with {} pins", x.0, x.1))
            .collect::<Vec<String>>()
            .join("\n")
    );

    Ok(Reply::new(response, ReplyType::Channel(msg.channel_id)))
}
