use crate::errors::Result;

use serenity::builder::ParseValue;
use serenity::model;
use serenity::model::channel::MessageReference;
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
    MessageId(model::id::MessageId, model::id::ChannelId),
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
            ReplyType::MessageId(msg_id, channel_id) => {
                // The following code is essentially entirely copied from serenity (the library being used)
                // codebase directly. It is licensed under ISC, I think it is fine to use it here. They
                // own the copyright, etc.alloc
                channel_id
                    .send_message(ctx, |builder| {
                        builder
                            .reference_message(MessageReference::from((*channel_id, *msg_id)))
                            .allowed_mentions(|f| {
                                f.replied_user(false)
                                    .parse(ParseValue::Everyone)
                                    .parse(ParseValue::Users)
                                    .parse(ParseValue::Roles)
                            });
                        builder.content(resp)
                    })
                    .await?;
            }
        }

        Ok(())
    }
}
