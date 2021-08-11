mod commands;
mod links;

use crate::errors::{Error, Result};
use serenity::{
    async_trait,
    http::Http,
    model::{
        channel::Message, channel::MessageType, gateway::Ready, guild::GuildStatus, id::GuildId,
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
    fn process_messages(&self, messages: Vec<Message>, db: &DB) -> Result<()> {
        if messages.len() > 0 {
            for msg in messages {
                if msg.author.bot {
                    continue;
                }

                let server_id = msg.guild_id.ok_or(Error::Internal(
                    "Guild/Server id required to be set on msg for processing".to_string(),
                ))?;
                log_error(
                    db.update_user(msg.author.id, server_id, &msg.author.name),
                    "Db update user",
                );
                if db.add_message(
                    msg.id,
                    *msg.channel_id.as_u64(),
                    *server_id.as_u64(),
                    msg.author.id,
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

    async fn get_channel_messages_with_query(
        &self,
        http: &Http,
        channel_id: u64,
        server_id: u64,
        query: &str,
    ) -> Result<Vec<Message>> {
        Ok(http
            .get_messages(channel_id, query)
            .await?
            .into_iter()
            .map(|mut msg| {
                if msg.guild_id.is_none() {
                    msg.guild_id = Some(GuildId(server_id));
                };
                msg
            })
            .collect())
    }

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

        self.process_messages(
            self.get_channel_messages_with_query(http, channel_id, server_id, &query)
                .await?,
            &db,
        )
    }

    async fn process_null_messages(
        &self,
        http: &Http,
        channel_id: u64,
        server_id: u64,
    ) -> Result<()> {
        const LIMIT: u64 = 50;
        let db = DB::get_db()?;
        let msg_id = db.get_null_user_message(channel_id)?;
        if msg_id.is_some() {
            println!("processing null msg around {}", msg_id.unwrap());
            self.process_messages(
                self.get_channel_messages_with_query(
                    http,
                    channel_id,
                    server_id,
                    &format!("?limit={}&around={}", LIMIT, msg_id.unwrap()),
                )
                .await?,
                &db,
            )
        } else {
            Ok(())
        }
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

        log_error(
            db.update_user(msg.author.id, server, &msg.author.name),
            "Db update user",
        );

        // get channel id and load message
        let channel_id = *msg.channel_id.as_u64();
        let channel_name = msg.channel_id.name(&ctx.cache).await;
        log_error(
            db.update_channel(channel_id, server_id, channel_name.unwrap()),
            "Db update channel",
        );

        log_error(
            db.add_message(msg.id, channel_id, server_id, msg.author.id),
            "Db add message",
        );

        if msg.kind == MessageType::Regular {
            self.check_links(&ctx, &msg).await;
            log_error(
                self.process_old_messages(&ctx.http, channel_id, server_id)
                    .await,
                "Process old messages",
            );
            log_error(
                self.process_null_messages(&ctx.http, channel_id, server_id)
                    .await,
                "Process null messages",
            );
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
