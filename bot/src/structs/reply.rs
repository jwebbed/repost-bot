use crate::errors::Result;

use db::{ReadOnlyDb, WriteableDb, read_only_db_call, writable_db_call};
use log::info;
use serenity::builder::{CreateAllowedMentions, CreateMessage, EditMessage};
use serenity::model::channel::{Message, MessageReference};
use serenity::model::id::{ChannelId, MessageId};
use serenity::prelude::Context;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyType {
    /// Send a standalone message in a channel
    Channel(ChannelId),
    /// Reply to a message, editing any previous reply to it instead of
    /// sending another
    Message(MessageId, ChannelId),
}

impl From<&Message> for ReplyType {
    fn from(msg: &Message) -> ReplyType {
        ReplyType::Message(msg.id, msg.channel_id)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Reply {
    message: Cow<'static, str>,
    place: ReplyType,
}

impl Reply {
    pub fn new(message: impl Into<Cow<'static, str>>, place: ReplyType) -> Reply {
        Reply {
            message: message.into(),
            place,
        }
    }

    pub async fn send(&self, ctx: &Context) -> Result<()> {
        match self.place {
            ReplyType::Channel(channel) => {
                channel.say(ctx, &*self.message).await?;
            }
            ReplyType::Message(msg_id, channel_id) => {
                if let Some(db_reply) = read_only_db_call(|db| db.get_reply(msg_id.get()))? {
                    info!("Editing reply w/ id {}", db_reply.id);
                    ChannelId::new(db_reply.channel)
                        .edit_message(
                            ctx,
                            MessageId::new(db_reply.id),
                            EditMessage::new().content(&*self.message),
                        )
                        .await?;
                } else {
                    let message_builder = CreateMessage::new()
                        .reference_message(MessageReference::from((channel_id, msg_id)))
                        .allowed_mentions(CreateAllowedMentions::new().replied_user(false))
                        .content(&*self.message);
                    let reply = channel_id.send_message(ctx, message_builder).await?;
                    writable_db_call(|db| {
                        db.add_reply(reply.id.get(), channel_id.get(), msg_id.get())
                    })?;
                }
            }
        }

        Ok(())
    }
}
