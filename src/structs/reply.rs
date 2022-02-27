use crate::errors::Result;

use serenity::model;
use serenity::prelude::Context;

#[derive(Debug)]
pub enum ReplyContents {
    String(String),
    ConstStr(&'static str),
}

#[derive(Debug)]
pub enum ReplyType<'a> {
    Message(&'a model::channel::Message),
    Channel(model::id::ChannelId),
}

#[derive(Debug)]
pub struct Reply<'a> {
    message: ReplyContents,
    place: ReplyType<'a>,
}

impl Reply<'_> {
    pub const fn new(message: String, place: ReplyType<'_>) -> Reply<'_> {
        Reply {
            message: ReplyContents::String(message),
            place,
        }
    }

    pub const fn new_const<'a>(message: &'static str, place: ReplyType<'a>) -> Reply<'a> {
        Reply {
            message: ReplyContents::ConstStr(message),
            place,
        }
    }

    pub async fn send(&self, ctx: &Context) -> Result<()> {
        let resp = match &self.message {
            ReplyContents::String(inner) => inner,
            ReplyContents::ConstStr(inner) => *inner,
        };

        match &self.place {
            ReplyType::Message(msg) => {
                msg.reply(ctx, resp).await?;
            }
            ReplyType::Channel(channel) => {
                channel.say(ctx, resp).await?;
            }
        }

        Ok(())
    }
}
