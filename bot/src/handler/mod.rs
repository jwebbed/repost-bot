mod commands;
mod images;
mod links;

use crate::errors::{Error, Result};
use crate::structs::reply::Reply;
use crate::structs::repost::RepostSet;

use db::{get_read_only_db, get_writeable_db, writable_db_call, ReadOnlyDb, WriteableDb};
use images::ImageProcesser;
use log::{debug, error, info, trace, warn};
use rand::seq::SliceRandom;
use rand::{random, thread_rng};

use serenity::{
    async_trait,
    cache::Cache,
    model::{
        channel::{Channel, ChannelType, GuildChannel, Message, MessageType},
        gateway::Ready,
        guild::Member,
        id::{ChannelId, GuildId, MessageId},
        permissions::Permissions,
        prelude::MessageUpdateEvent,
    },
    prelude::*,
};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub struct Handler;

#[inline(always)]
pub fn log_error<T>(r: rusqlite::Result<T>, label: &str) {
    match r {
        Ok(_) => (),
        Err(why) => error!("{label} failed with error: {why:?}"),
    }
}

#[inline(always)]
fn regular_text_msg(kind: MessageType) -> bool {
    kind == MessageType::Regular || kind == MessageType::InlineReply
}

pub async fn bot_read_channel_permission(cache: impl AsRef<Cache>, channel: &GuildChannel) -> bool {
    let current_user_id = cache.as_ref().current_user().id;
    match channel.permissions_for_user(cache, current_user_id) {
        Ok(permissions) => {
            permissions.contains(Permissions::READ_MESSAGE_HISTORY | Permissions::VIEW_CHANNEL)
        }
        Err(_why) => false,
    }
}

/// takes the message from discord, stores it, and returns the db struct for further processing
async fn process_discord_message(ctx: &Context, msg: &Message) -> Result<db::structs::Message> {
    if msg.author.bot {
        return Err(Error::BotMessage);
    }

    if !regular_text_msg(msg.kind) {
        return Err(Error::ConstStr("Message is not a regular text message"));
    }
    let now = Instant::now();

    let db = get_writeable_db()?;

    let author_id = *msg.author.id.as_u64();
    db.add_user(
        author_id,
        &msg.author.name,
        msg.author.bot,
        msg.author.discriminator,
    )?;

    let server = msg
        .guild_id
        .ok_or(Error::ConstStr("Guild id doesn't exist on message"))?;
    let server_id = *server.as_u64();
    let server_name = &server.name(ctx);
    db.update_server(server_id, server_name)?;

    // get channel id and load message
    let channel_id = *msg.channel_id.as_u64();
    let channel_name = msg.channel_id.name(&ctx.cache).await;
    // we can assume channel is visible if we are receiving messages for it
    db.update_channel(channel_id, server_id, &channel_name.unwrap(), true)?;

    let ret = db.add_message(msg.id, channel_id, server_id, author_id)?;

    trace!(
        "process_discord_message time elapsed: {:.2?}",
        now.elapsed()
    );

    Ok(ret)
}

async fn process_message_update<'a>(
    _ctx: &Context,
    _old_if_available: &Option<Message>,
    _new: &Option<Message>,
    event: &'a MessageUpdateEvent,
) -> Result<Option<Reply<'a>>> {
    let msg_id = *event.id.as_u64();
    if event.guild_id.is_none() {
        warn!("Received message update on msg_id {msg_id} with no guild_id, can't process");
        return Ok(None);
    }
    let db_msg_maybe = get_read_only_db()?.get_message(event.id)?;
    if db_msg_maybe.is_none() {
        warn!("Received message update on msg_id {msg_id} but haven't already processed message, can't process");
        return Ok(None);
    }
    // TODO: no more unwraps
    let db_msg = db_msg_maybe.unwrap();
    // just handling embeds right now as it's a common occurance that the embed
    // only gets loaded after the message is first sent. As such, if we don't
    // handle it, embeds will get routinely missed.
    //
    // We should eventually use this to check if existing images / links are
    // removed and if attachments / links are added.
    if event.embeds.is_some() || event.attachments.is_some() {
        // we should reply if the message is recent, if it's an older message
        // being updated we'll leave it be
        let should_reply = db_msg.is_recent();

        let embeds_default = vec![];
        let attachments_default = vec![];

        let embeds = event.embeds.as_ref().map_or(&embeds_default, |r| r);
        let attachments = event
            .attachments
            .as_ref()
            .map_or(&attachments_default, |r| r);

        let mut reposts = ImageProcesser::new(
            msg_id,
            *event.guild_id.unwrap().as_u64(),
            attachments,
            embeds,
        )
        .process(should_reply)
        .await?;
        if should_reply && reposts.len() > 0 {
            // need to get any link reposts if we're gonna edit the reply
            reposts.union(&links::get_reposts_for_message_id(msg_id)?);
            return Ok(reposts.generate_reply_for_message_id(
                &event.id,
                &event.channel_id,
                db_msg.created_at,
            ));
        }
    }

    Ok(None)
}

async fn process_message<'a>(
    ctx: &Context,
    msg: &'a Message,
    new: bool,
) -> Result<Option<Reply<'a>>> {
    // need to do this first, also does validation
    let db_msg = process_discord_message(ctx, msg).await?;

    let ret = if commands::has_command_prefix(&msg.content) {
        if new {
            commands::handle_command(ctx, msg).await
        } else {
            None
        }
    } else {
        let mut repost_set = RepostSet::new();
        if !db_msg.is_embed_parsed() {
            repost_set.union(&ImageProcesser::from_message(msg)?.process(new).await?);
        };

        if !db_msg.is_repost_parsed() {
            repost_set.union(&links::store_links_and_get_reposts(msg, new)?);
        };

        repost_set.generate_reply_for_message(msg)
    };

    get_writeable_db()?.mark_message_all_checked(msg.id)?;

    Ok(ret)
}

/// takes the message from discord and does slow, less important operations
async fn process_discord_message_slow(ctx: &Context, msg: &Message) -> Result<()> {
    let server_id = msg
        .guild_id
        .ok_or_else(|| Error::ConstStr("Guild id doesn't exist"))?;

    if let Some(nickname) = msg.author.nick_in(ctx, server_id).await {
        let db = get_writeable_db()?;
        db.add_nickname(*msg.author.id.as_u64(), *server_id.as_u64(), &nickname)?;
    }

    Ok(())
}

async fn process_old_messages(ctx: &Context, server_id: &u64) -> Result<usize> {
    const LIMIT: u64 = 50;
    let db = get_read_only_db()?;
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
    let db = get_writeable_db()?;
    if !messages.is_empty() {
        let len = messages.len();
        info!("received {len} messages for channel id: {channel_id} and query_string {query}");
        let mut ids = HashSet::with_capacity(len);
        for mut msg in messages {
            let id = *msg.id.as_u64();
            ids.insert(id);

            if msg.author.bot || !regular_text_msg(msg.kind) {
                if let Some(base_id) = base_msg {
                    if base_id == id {
                        warn!("base msg id {id} either a bot or not a regular text message");
                        db.soft_delete_message(base_id)?;
                    }
                }
            } else {
                let db_msg_maybe = db.get_message(msg.id)?;
                if msg.guild_id.is_none() {
                    msg.guild_id = Some(GuildId(*server_id));
                }
                if let Err(why) = process_message(ctx, &msg, false).await {
                    warn!("Failed to process old message {} with error {why:?}", id);
                }

                if let Some(db_msg) = db_msg_maybe {
                    if !db_msg.is_deleted() && !db_msg.is_checked_old() {
                        // mark as checked old if we had this in the db before processing just now
                        db.mark_message_checked_old(msg.id)?;
                    }
                } else {
                    debug!(
                        "message {} not already in db, must have been sent whilst server down",
                        msg.id
                    );
                }
            }
        }

        // if when querying around ID the ID itself doesn't show up,
        // this would seem to indicate it was deleted and we missed it.
        // As this is unclear, we just soft delete it.
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
                        error!("message: failed to send reply {why:?}");
                    }
                }

                if let Err(why) = process_discord_message_slow(&ctx, &msg).await {
                    error!("message: failed process discord messages slow with error: {why:?}");
                }
            }
            Err(why) => match why {
                Error::BotMessage => debug!("Skipped processing bot message"),
                _ => error!("message: failed to process messsage: {why:?}"),
            },
        }
    }

    async fn message_update(
        &self,
        ctx: Context,
        old_if_available: Option<Message>,
        new: Option<Message>,
        event: MessageUpdateEvent,
    ) {
        info!("received message update on {new:?} (old: {old_if_available:?}) w/ event {event:?}");
        match process_message_update(&ctx, &old_if_available, &new, &event).await {
            Ok(result) => {
                if let Some(reply) = result {
                    if let Err(why) = reply.send(&ctx).await {
                        error!("message_update: failed to send reply {why:?}");
                    }
                }
            }
            Err(why) => error!("message_update: failed to process messsage: {why:?}"),
        }
    }

    async fn message_delete(
        &self,
        _ctx: Context,
        _channel_id: ChannelId,
        message_id: MessageId,
        _guild_id: Option<GuildId>,
    ) {
        let db = match get_writeable_db() {
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
            writable_db_call(|db| {
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
                    writable_db_call(|db| db.update_channel_visibility(channel.id, visible)),
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
            writable_db_call(|db| db.delete_channel(channel.id)),
            "Db delete channel",
        );
    }

    async fn guild_member_update(
        &self,
        _ctx: Context,
        _old_if_available: Option<Member>,
        new: Member,
    ) {
        let db = match get_writeable_db() {
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
        let db = match get_writeable_db() {
            Ok(db) => db,
            Err(why) => {
                error!("Error getting db: {why:?}");
                return;
            }
        };

        let ctx: &'static Context = Box::leak(Box::new(ctx));
        for guild in guilds {
            let server_name = guild.name(ctx);

            log_error(
                db.update_server(*guild.as_u64(), &server_name),
                "Update server name from cache_ready",
            );

            let g = *guild.as_u64();
            tokio::spawn(async move {
                loop {
                    let tts = match process_old_messages(ctx, &g).await {
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
                    trace!("process old message task sleeping {tts}s");
                    tokio::time::sleep(Duration::from_secs(tts)).await;
                }
            });

            match guild.channels(ctx).await {
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
                        Ok(cs) => HashMap::from_iter(cs),
                        Err(_why) => HashMap::new(),
                    };
                    for (id, name) in channels_stored.clone() {
                        if !channels.contains_key(&id) {
                            warn!("stored channel {name} with id {id} no longer exists on server, deleting");
                            log_error(db.delete_channel(id), "Db delete channel");
                        }
                    }

                    // insert all channels to update names and build visibility map
                    let mut visibility_map = HashMap::with_capacity(channels.len());
                    for (id, channel) in channels.clone() {
                        let visible = bot_read_channel_permission(ctx, &channel).await;
                        visibility_map.insert(id, visible);
                        log_error(
                            db.update_channel(
                                *channel.id.as_u64(),
                                *channel.guild_id.as_u64(),
                                &channel.name,
                                visible,
                            ),
                            "Db update channel",
                        );
                    }

                    // check for most recent message
                    for id in channels
                        .keys()
                        .filter(|id| *visibility_map.get(id).unwrap_or(&true))
                        .map(|id| *id.as_u64())
                    {
                        match ctx.http.get_messages(id, "?limit=1").await {
                            Ok(mut msg_vec) => {
                                if let Some(msg) = msg_vec.pop() {
                                    if !msg.author.bot {
                                        log_error(
                                            db.add_message(
                                                msg.id,
                                                *msg.channel_id.as_u64(),
                                                *guild.as_u64(),
                                                *msg.author.id.as_u64(),
                                            ),
                                            "db add message",
                                        );
                                    }
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
