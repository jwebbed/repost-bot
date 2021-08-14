mod commands;
mod links;

use crate::errors::Result;
use serenity::{
    async_trait,
    http::Http,
    model::{
        channel::{Channel, GuildChannel, Message, MessageType},
        gateway::Ready,
        guild::GuildStatus,
        id::{ChannelId, GuildId, MessageId},
        permissions::Permissions,
    },
    prelude::*,
};

use crate::db::DB;

pub struct Handler;

pub fn log_error<T>(r: Result<T>, label: &str) {
    match r {
        Ok(_) => (),
        Err(why) => println!("{} failed with error: {:?}", label, why),
    }
}
impl Handler {
    async fn process_old_messages(
        &self,
        http: &Http,
        channel_id: u64,
        server_id: u64,
    ) -> Result<()> {
        const LIMIT: u64 = 50;
        let db = DB::get_db()?;
        let query = match db.get_oldest_message(channel_id)? {
            Some(value) => format!("?limit={}&before={}", LIMIT, value),
            None => format!("?limit={}", LIMIT),
        };

        let messages = http.get_messages(channel_id, &query).await?;
        if messages.len() > 0 {
            for mut msg in messages {
                if msg.author.bot {
                    continue;
                }
                if msg.guild_id.is_none() {
                    msg.guild_id = Some(GuildId(server_id));
                }
                if db.add_message(
                    msg.id,
                    *msg.channel_id.as_u64(),
                    *msg.guild_id.unwrap().as_u64(),
                )? {
                    self.store_links_and_get_reposts(&msg);
                } else {
                    println!("message {} already processed", msg.id.as_u64());
                }
            }
        } else {
            println!("received no messages to process")
        }

        Ok(())
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        // dont care about bot messages
        if msg.author.bot {
            return;
        }

        if msg.guild_id.is_none() {
            println!("Guild id doesn't exist, for now we don't care about this");
            return;
        }

        if msg.content.starts_with("!rpm") {
            self.handle_command(&ctx, &msg).await;
            return;
        }

        let db = match DB::get_db() {
            Ok(db) => db,
            Err(why) => {
                println!("Error getting db: {:?}", why);
                return;
            }
        };

        let server = msg.guild_id.unwrap();
        let server_id = *server.as_u64();
        let server_name = server.name(&ctx.cache).await;
        log_error(
            db.update_server(server_id, &server_name),
            "Db update server",
        );

        // get channel id and load message
        let channel_id = *msg.channel_id.as_u64();
        let channel_name = msg.channel_id.name(&ctx.cache).await;
        log_error(
            db.update_channel(channel_id, server_id, &channel_name.unwrap()),
            "Db update channel",
        );

        log_error(
            db.add_message(msg.id, channel_id, server_id),
            "Db add message",
        );

        if msg.kind == MessageType::Regular {
            self.check_links(&ctx, &msg).await;
            log_error(
                self.process_old_messages(&ctx.http, channel_id, server_id)
                    .await,
                "Process old messages",
            );
        }
    }

    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
        let db = match DB::get_db() {
            Ok(db) => db,
            Err(why) => {
                println!("Error getting db: {:?}", why);
                return;
            }
        };

        match db.delete_message(message_id) {
            Ok(_) => println!(
                "successfully deleted message id {} from db",
                *message_id.as_u64()
            ),
            Err(why) => println!(
                "failed to delete message id {} with following error {:?}",
                message_id.as_u64(),
                why
            ),
        };
    }

    async fn channel_create(&self, _ctx: Context, channel: &GuildChannel) {
        log_error(
            DB::db_call(|db| {
                db.update_channel(
                    *channel.id.as_u64(),
                    *channel.guild_id.as_u64(),
                    &channel.name,
                )
            }),
            "Db update channel",
        );
    }

    async fn channel_update(&self, ctx: Context, _old: Option<Channel>, new: Channel) {
        match new.guild() {
            Some(channel) => {
                let current_user_id = ctx.cache.current_user().await.id;
                if let Ok(permissions) = channel
                    .permissions_for_user(&ctx.cache, current_user_id)
                    .await
                {
                    println!(
                        "current user has the following permissions {:?}",
                        permissions
                    );

                    log_error(
                        DB::db_call(|db| {
                            db.update_channel_visibility(
                                channel.id,
                                permissions.contains(Permissions::READ_MESSAGES),
                            )
                        }),
                        "Updating visibility",
                    );
                }
            }
            None => {
                println!("It's not a guild!");
            }
        }
    }
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let db = match DB::get_db() {
            Ok(db) => db,
            Err(why) => {
                println!("Error getting db: {:?}", why);
                return;
            }
        };

        for guild_id in ready.guilds {
            log_error(
                match guild_id {
                    GuildStatus::OnlineGuild(g) => db.update_server(*g.id.as_u64(), &Some(g.name)),
                    GuildStatus::OnlinePartialGuild(g) => {
                        db.update_server(*g.id.as_u64(), &Some(g.name))
                    }
                    GuildStatus::Offline(g) => db.update_server(*g.id.as_u64(), &None),
                    _ => Ok(()),
                },
                "db update server",
            );
        }
    }
}
