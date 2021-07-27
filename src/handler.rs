use linkify::{LinkFinder, LinkKind};
use rusqlite::Result;

use serenity::{
    async_trait,
    model::{channel::Message, channel::MessageType, gateway::Ready, guild::GuildStatus},
    prelude::*,
};

use crate::db::get_db;
use crate::structs::Link;

pub struct Handler {
    finder: LinkFinder,
}

impl Handler {
    pub fn new() -> Handler {
        let mut finder = LinkFinder::new();
        finder.kinds(&[LinkKind::Url]);

        Handler { finder }
    }

    fn get_links(&self, msg: &str) -> Vec<String> {
        self.finder
            .links(msg)
            .map(|x| x.as_str().to_string())
            .collect()
    }
}

fn log_error<T>(r: Result<T>, label: String) {
    match r {
        Ok(_) => (),
        Err(why) => println!("{} failed with error: {:?}", label, why),
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

        let db = match get_db() {
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
            "Db update server".to_string(),
        );

        // get channel id and load message
        let channel_id = *msg.channel_id.as_u64();
        let channel_name = msg.channel_id.name(&ctx.cache).await;
        log_error(
            db.update_channel(channel_id, server_id, channel_name.unwrap()),
            "Db update channel".to_string(),
        );

        let message_id = *msg.id.as_u64();
        log_error(
            db.add_message(message_id, channel_id, server_id),
            "Db add message".to_string(),
        );

        if msg.kind == MessageType::Regular {
            let mut repost = false;
            for link in self.get_links(&msg.content) {
                repost |= match db.query_links(&link, server_id) {
                    Ok(reposts) => {
                        println!("Found {} reposts: {:?}", reposts.len(), reposts);
                        reposts.len() > 0
                    }
                    Err(why) => {
                        println!("Failed to load reposts with err: {:?}", why);
                        false
                    }
                };

                let l = Link {
                    link: link,
                    server: server_id,
                    channel: *msg.channel_id.as_u64(),
                    message: *msg.id.as_u64(),
                    ..Default::default()
                };

                log_error(db.insert_link(l), "Insert link".to_string());
            }

            if repost {
                match msg.reply(&ctx.http, "REPOST").await {
                    Ok(_) => (),
                    Err(why) => println!("Failed to inform of REPOST: {:?}", why),
                }
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
        let db = match get_db() {
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
                "db update server".to_string(),
            );
        }
    }
}
