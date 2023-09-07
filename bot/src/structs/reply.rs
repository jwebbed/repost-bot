use db::{read_only_db_call, writable_db_call, ReadOnlyDb, WriteableDb};
use log::info;
use processers::{Error, Result};
use serde_json::json;
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
            ReplyType::Channel(channel) => {
                channel.say(ctx, resp).await?;
            }
            ReplyType::Message(msg) => {
                if let Some(db_reply) = read_only_db_call(|db| db.get_reply(*msg.id.as_u64()))? {
                    edit_reply(ctx, &db_reply, resp).await?;
                } else {
                    let reply = msg.reply(ctx, resp).await?;
                    self.store_reply(reply.id)?;
                }
            }
            ReplyType::MessageId(msg_id, channel_id) => {
                if let Some(db_reply) = read_only_db_call(|db| db.get_reply(*msg_id.as_u64()))? {
                    edit_reply(ctx, &db_reply, resp).await?;
                } else {
                    // The following code is essentially entirely copied from serenity (the library being used)
                    // codebase directly. It is licensed under ISC, I think it is fine to use it here. They
                    // own the copyright, etc.alloc
                    let reply = channel_id
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
                    self.store_reply(reply.id)?;
                }
            }
        };

        Ok(())
    }

    fn store_reply(&self, reply_id: model::id::MessageId) -> Result<()> {
        let (replied_to, channel_id) = match &self.place {
            ReplyType::Message(msg) => Ok((*msg.id.as_u64(), *msg.channel_id.as_u64())),
            ReplyType::MessageId(msg_id, channel_id) => {
                Ok((*msg_id.as_u64(), *channel_id.as_u64()))
            }
            _ => Err(Error::ConstStr(
                "Can't store reply if not replying to a message",
            )),
        }?;

        writable_db_call(|db| db.add_reply(*reply_id.as_u64(), channel_id, replied_to))?;
        Ok(())
    }
}

async fn edit_reply(ctx: &Context, db_reply: &db::structs::Reply, content: &str) -> Result<()> {
    info!("Editing reply w/ id {}", db_reply.id);
    ctx.http
        .edit_message(
            db_reply.channel,
            db_reply.id,
            &json!({ "content": content }),
        )
        .await?;
    Ok(())
}
