mod commands;
mod links;
mod wordle;

use crate::db::DB;
use crate::errors::{Error, Result};
use crate::structs;
use crate::structs::reply::Reply;

use log::{debug, error, info, trace, warn};
use rand::seq::SliceRandom;
use rand::{random, thread_rng};
use serenity::{
    async_trait,
    model::{
        channel::{Channel, ChannelType, GuildChannel, Message, MessageType},
        gateway::Ready,
        guild::Member,
        id::{ChannelId, GuildId, MessageId},
        permissions::Permissions,
    },
    prelude::*,
};
use std::collections::HashMap;
use std::collections::HashSet;
use std::time::Instant;
use std::{sync::Arc, time::Duration};

pub struct Handler;

#[inline(always)]
pub fn log_error<T>(r: Result<T>, label: &str) {
    match r {
        Ok(_) => (),
        Err(why) => error!("{label} failed with error: {why:?}"),
    }
}

async fn bot_read_channel_permission(ctx: &Context, channel: &GuildChannel) -> bool {
    let current_user_id = ctx.cache.current_user().await.id;
    match channel
        .permissions_for_user(&ctx.cache, current_user_id)
        .await
    {
        Ok(permissions) => permissions.contains(Permissions::READ_MESSAGES),
        Err(_why) => false,
    }
}

/// takes the message from discord, stores it, and returns the db struct for further processing
async fn process_discord_message(ctx: &Context, msg: &Message) -> Result<structs::link::Message> {
    if msg.author.bot {
        return Err(Error::BotMessage);
    }

    if msg.kind != MessageType::Regular {
        return Err(Error::ConstStr("Message is not a regular text message"));
    }
    let now = Instant::now();

    let db = DB::get_db()?;

    let author_id = *msg.author.id.as_u64();
    db.add_user(
        author_id,
        &msg.author.name,
        msg.author.bot,
        msg.author.discriminator,
    )?;

    let server = msg
        .guild_id
        .ok_or_else(|| Error::ConstStr("Guild id doesn't exist on message"))?;
    let server_id = *server.as_u64();
    let server_name = &server.name(&ctx).await;
    db.update_server(server_id, server_name)?;

    // get channel id and load message
    let channel_id = *msg.channel_id.as_u64();
    let channel_name = msg.channel_id.name(&ctx.cache).await;
    // we can assume channel is visible if we are receiving messages for it
    db.update_channel(channel_id, server_id, &channel_name.unwrap(), true)?;

    let ret = db.add_message(msg.id, channel_id, server_id, author_id);

    debug!(
        "process_discord_message time elapsed: {:.2?}",
        now.elapsed()
    );

    ret
}

async fn process_message<'a>(
    ctx: &Context,
    msg: &'a Message,
    new: bool,
) -> Result<Option<Reply<'a>>> {
    // need to do this first, also does validation
    let db_msg = process_discord_message(ctx, msg).await?;

    let ret = if msg.content.starts_with("!rpm") {
        if new {
            commands::handle_command(ctx, msg).await
        } else {
            None
        }
    } else {
        if !db_msg.parsed_wordle {
            wordle::check_wordle(msg);
        }

        // return the reply option from parsing reposts
        if !db_msg.parsed_repost {
            links::store_links_and_get_reposts(msg)?
        } else {
            None
        }
    };

    DB::db_call(|db| db.mark_message_all_checked(msg.id))?;

    Ok(ret)
}

/// takes the message from discord and does slow, less important operations
async fn process_discord_message_slow(ctx: &Context, msg: &Message) -> Result<()> {
    let server_id = msg
        .guild_id
        .ok_or_else(|| Error::Internal("Guild id doesn't exist".to_string()))?;

    if let Some(nickname) = msg.author.nick_in(ctx, server_id).await {
        let db = DB::get_db()?;
        db.add_nickname(*msg.author.id.as_u64(), *server_id.as_u64(), &nickname)?;
    }

    Ok(())
}

async fn process_old_messages(ctx: &Context, server_id: &u64) -> Result<usize> {
    const LIMIT: u64 = 50;
    let db = DB::get_db()?;
    let (channel_id, query, base_msg) = match db.get_newest_unchecked_message(*server_id)? {
        Some(msg) => (
            msg.channel,
            format!("?limit={LIMIT}&around={}", msg.id),
            Some(msg.id),
        ),
        None => {
            // if there is nothing to query we really don't need to spam the api all the time
            if random::<f64>() > 0.015 {
                return Ok(0);
            }
            let channels = db.get_known_channels(*server_id)?;
            let mut rng = thread_rng();
            let channel = channels
                .choose(&mut rng)
                .ok_or(Error::ConstStr("Rng choose failed for some reason"))?;

            (channel.id, format!("?limit={LIMIT}"), None)
        }
    };

    let messages = ctx.http.get_messages(channel_id, &query).await?;
    if !messages.is_empty() {
        let len = messages.len();
        info!("received {len} messages for channel id: {channel_id} and query_string {query}");
        let mut ids = HashSet::with_capacity(len);
        for mut msg in messages {
            ids.insert(*msg.id.as_u64());

            if msg.author.bot || msg.kind != MessageType::Regular {
                continue;
            }
            if msg.guild_id.is_none() {
                msg.guild_id = Some(GuildId(*server_id));
            }
            if let Err(why) = process_message(ctx, &msg, false).await {
                warn!(
                    "Failed to process old message {} with error {why:?}",
                    msg.id
                );
            }
        }

        if let Some(base_msg) = base_msg {
            if !ids.contains(&base_msg) {
                warn!("did not received base msg id {base_msg} when querying for messages");
                db.soft_delete_message(base_msg)?;
            }
        }

        Ok(len)
    } else {
        debug!("received no messages to process");
        Ok(0)
    }
}

impl Handler {
    pub const fn new() -> Handler {
        Handler {}
    }
}

#[async_trait]
impl EventHandler for Handler {
    async fn message(&self, ctx: Context, msg: Message) {
        match process_message(&ctx, &msg, true).await {
            Ok(result) => {
                if let Some(reply) = result {
                    if let Err(why) = reply.send(&ctx).await {
                        error!("failed to send reply {why:?}");
                    }
                }

                if let Err(why) = process_discord_message_slow(&ctx, &msg).await {
                    error!("failed process discord messages slow with error: {why:?}");
                }
            }
            Err(why) => error!("failed to process messsage: {why:?}"),
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
                error!("Error getting db: {why:?}");
                return;
            }
        };

        match db.delete_message(message_id) {
            Ok(_) => info!(
                "successfully deleted message id {} from db",
                *message_id.as_u64()
            ),
            Err(why) => error!(
                "failed to delete message id {} with following error {:?}",
                message_id.as_u64(),
                why
            ),
        };
    }

    async fn channel_create(&self, ctx: Context, channel: &GuildChannel) {
        let visible = bot_read_channel_permission(&ctx, channel).await;
        log_error(
            DB::db_call(|db| {
                db.update_channel(
                    *channel.id.as_u64(),
                    *channel.guild_id.as_u64(),
                    &channel.name,
                    visible,
                )
            }),
            "Db update channel",
        );
    }

    async fn channel_update(&self, ctx: Context, _old: Option<Channel>, new: Channel) {
        match new.guild() {
            Some(channel) => {
                let visible = bot_read_channel_permission(&ctx, &channel).await;
                let (id, name, server) = (channel.id, channel.name, *channel.guild_id.as_u64());
                info!("received channel update for channel id {id} with name {name} in server {server}, visibility is now: {visible}");
                log_error(
                    DB::db_call(|db| db.update_channel_visibility(channel.id, visible)),
                    "Updating visibility",
                );
            }
            None => {
                warn!("It's not a guild!");
            }
        }
    }

    async fn channel_delete(&self, _ctx: Context, channel: &GuildChannel) {
        trace!("recieved channel delete for {channel:?}");
        log_error(
            DB::db_call(|db| db.delete_channel(channel.id)),
            "Db delete channel",
        );
    }

    async fn guild_member_update(
        &self,
        _ctx: Context,
        _old_if_available: Option<Member>,
        new: Member,
    ) {
        let db = match DB::get_db() {
            Ok(db) => db,
            Err(why) => {
                error!("Error getting db: {why:?}");
                return;
            }
        };
        let author_id = *new.user.id.as_u64();
        if let Err(why) = db.add_user(
            author_id,
            &new.user.name,
            new.user.bot,
            new.user.discriminator,
        ) {
            error!("Error adding user: {why:?}");
            return;
        }

        if let Some(nickname) = new.nick {
            if let Err(why) = db.add_nickname(author_id, *new.guild_id.as_u64(), &nickname) {
                error!("Error adding nickname: {why:?}");
                return;
            }
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        let db = match DB::get_db() {
            Ok(db) => db,
            Err(why) => {
                error!("Error getting db: {why:?}");
                return;
            }
        };
        let ctx = Arc::new(ctx);
        for guild in guilds {
            let ctxn = Arc::clone(&ctx);
            let g = Arc::new(*guild.as_u64());
            tokio::spawn(async move {
                loop {
                    let g1 = Arc::clone(&g);
                    let tts = match process_old_messages(&Arc::clone(&ctxn), &g1).await {
                        Ok(val) => {
                            if val == 0 {
                                10 * 60
                            } else {
                                45
                            }
                        }
                        Err(why) => {
                            warn!("process old messages with err {why:?}");
                            240
                        }
                    };
                    tokio::time::sleep(Duration::from_secs(tts)).await;
                }
            });

            match guild.channels(&ctx).await {
                Ok(all_channels) => {
                    let mut mchannels = HashMap::new();
                    for (k, v) in all_channels {
                        if v.kind != ChannelType::Voice && v.kind != ChannelType::Category {
                            mchannels.insert(k, v);
                        }
                    }
                    // no longer mutable
                    let channels = mchannels;

                    let channel_list = channels
                        .values()
                        .map(|c| String::from(c.name.as_str()))
                        .collect::<Vec<String>>();

                    info!("found server with id {guild} and channels {channel_list:?}");
                    let channels_stored = match db.get_channel_list(guild) {
                        Ok(cs) => cs,
                        Err(_why) => Vec::new(),
                    };

                    for (id, name) in channels_stored {
                        if !channel_list.contains(&name) {
                            warn!(
                                "stored channel {} no longer exists on server, deleting",
                                name
                            );
                            log_error(db.delete_channel(id), "Db delete channel");
                        }
                    }

                    // check for most recent message
                    for id in channels.keys().map(|id| *id.as_u64()) {
                        match ctx.http.get_messages(id, "?limit=1").await {
                            Ok(msg) => {
                                if msg.len() > 0 && !msg[0].author.bot {
                                    log_error(
                                        db.add_message(
                                            msg[0].id,
                                            *msg[0].channel_id.as_u64(),
                                            *guild.as_u64(),
                                            *msg[0].author.id.as_u64(),
                                        ),
                                        "db add message",
                                    );
                                }
                            }
                            Err(why) => {
                                warn!("failed to load most recent message for id {id} {why:?}")
                            }
                        }
                    }
                }
                Err(why) => error!(
                    "failed to load channels for guild {} with error {why:?}",
                    *guild.as_u64()
                ),
            }
        }
    }
}
