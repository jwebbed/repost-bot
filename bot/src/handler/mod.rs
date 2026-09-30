mod commands;
mod images;
mod links;

use crate::errors::{Error, Result};
use crate::structs::reply::Reply;
use crate::structs::repost::RepostSet;
use crate::structs::{Post, PostProcessor, ProcessedPost};

use db::{ReadOnlyDb, WriteableDb, get_read_only_db, get_writeable_db, writable_db_call};
use futures_util::future::join_all;
use images::ImageProcessor;
use links::LinkProcessor;
use log::{debug, error, info, trace, warn};
use rand::rngs::SmallRng;
use rand::seq::IndexedRandom;
use rand::{Rng, RngExt, SeedableRng};
use serenity::all::GuildMemberUpdateEvent;
use serenity::{
    async_trait,
    builder::GetMessages,
    cache::Cache,
    model::{
        channel::{ChannelType, GuildChannel, Message, MessageType},
        gateway::Ready,
        guild::Member,
        id::{ChannelId, GuildId, MessageId},
        permissions::Permissions,
        prelude::MessageUpdateEvent,
    },
    prelude::*,
};
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Handler {
    /// Guilds that already have a task backfilling old messages, as
    /// cache_ready can be received more than once
    backfill_guilds: Mutex<HashSet<GuildId>>,
}

#[inline]
pub fn log_error<T, E: Debug>(r: std::result::Result<T, E>, label: &str) {
    if let Err(why) = r {
        error!("{label} failed with error: {why:?}");
    }
}

#[inline]
fn regular_text_msg(kind: MessageType) -> bool {
    kind == MessageType::Regular || kind == MessageType::InlineReply
}

/// Returns true if the bot can both see and read the history of a channel
pub fn bot_read_channel_permission(cache: impl AsRef<Cache>, channel: &GuildChannel) -> bool {
    let cache = cache.as_ref();
    let current_user_id = cache.current_user().id;
    let Some(guild) = cache.guild(channel.guild_id) else {
        return false;
    };
    guild.members.get(&current_user_id).is_some_and(|member| {
        guild
            .user_permissions_in(channel, member)
            .contains(Permissions::READ_MESSAGE_HISTORY | Permissions::VIEW_CHANNEL)
    })
}

/// takes the message from discord, stores it, and returns the db struct for further processing
async fn process_discord_message(ctx: &Context, msg: &Message) -> Result<Post> {
    if msg.author.bot {
        return Err(Error::BotMessage);
    }

    if !regular_text_msg(msg.kind) {
        return Err(Error::ConstStr("Message is not a regular text message"));
    }
    let server = msg
        .guild_id
        .ok_or(Error::ConstStr("Guild id doesn't exist on message"))?;
    let now = Instant::now();

    let channel_name = msg.channel_id.name(ctx).await;

    let db = get_writeable_db()?;

    db.add_user(msg.author.id.get(), &msg.author.name, msg.author.bot)?;

    let server_id = server.get();
    db.update_server(server_id, server.name(ctx).as_deref())?;

    let channel_id = msg.channel_id.get();
    match channel_name {
        // we can assume channel is visible if we are receiving messages for it
        Ok(name) => db.update_channel(channel_id, server_id, &name, true)?,
        Err(why) => warn!("failed to get name of channel {channel_id}: {why:?}"),
    }

    let db_msg = db.add_message(msg.id.get(), channel_id, server_id, msg.author.id.get())?;

    trace!(
        "process_discord_message time elapsed: {:.2?}",
        now.elapsed()
    );

    Ok(Post::from_message(&db_msg, msg))
}

/// Processes and stores a post, adding any reposts found to `reposts` when provided
async fn process_post<P: PostProcessor>(
    post: &Post,
    reposts: Option<&mut RepostSet>,
) -> Result<()> {
    let processed = P::process(post).await?;
    if let Some(reposts) = reposts {
        reposts.union(&processed.get_reposts()?);
    }
    processed.store_post()
}

async fn process_message_update(event: &MessageUpdateEvent) -> Result<Option<Reply>> {
    let msg_id = event.id.get();
    if event.guild_id.is_none() {
        warn!("Received message update on msg_id {msg_id} with no guild_id, can't process");
        return Ok(None);
    }
    let Some(db_msg) = get_read_only_db()?.get_message(msg_id)? else {
        warn!(
            "Received message update on msg_id {msg_id} but haven't already processed message, can't process"
        );
        return Ok(None);
    };
    let post = Post::from_update(&db_msg, event);
    if !post.has_attachments() {
        return Ok(None);
    }

    let mut reposts = RepostSet::default();
    process_post::<ImageProcessor>(&post, Some(&mut reposts)).await?;
    if post.db_message.is_recent() && !reposts.is_empty() {
        reposts.union(&links::get_reposts_for_message_id(post.id())?);
        return Ok(reposts.generate_reply_for_message_id(
            event.id,
            event.channel_id,
            post.db_message.created_at,
        ));
    }

    Ok(None)
}

async fn process_message(ctx: &Context, msg: &Message, new: bool) -> Result<Option<Reply>> {
    // need to do this first, also does validation
    let post = process_discord_message(ctx, msg).await?;
    info!("Received post: {post:?}");
    let ret = if let Some(command) = commands::parse_command(post.content()) {
        if new {
            commands::handle_command(ctx, msg, command).await
        } else {
            None
        }
    } else {
        // reposts are only reported for new messages, but old ones still need storing
        let mut repost_set = RepostSet::default();
        if !post.db_message.is_embed_parsed() {
            process_post::<ImageProcessor>(&post, new.then_some(&mut repost_set)).await?;
        }
        if !post.db_message.is_repost_parsed() {
            process_post::<LinkProcessor>(&post, new.then_some(&mut repost_set)).await?;
        }
        repost_set.generate_reply_for_message(msg)
    };

    get_writeable_db()?.mark_message_all_checked(msg.id.get())?;

    Ok(ret)
}

/// takes the message from discord and does slow, less important operations
async fn process_discord_message_slow(ctx: &Context, msg: &Message) -> Result<()> {
    let server_id = msg
        .guild_id
        .ok_or(Error::ConstStr("Guild id doesn't exist"))?;

    if let Some(nickname) = msg.author.nick_in(ctx, server_id).await {
        get_writeable_db()?.add_nickname(msg.author.id.get(), server_id.get(), &nickname)?;
    }

    Ok(())
}

async fn process_old_messages(
    ctx: &Context,
    server_id: u64,
    rng: &mut (impl Rng + ?Sized),
) -> Result<usize> {
    const LIMIT: u8 = 50;
    let (channel_id, query, base_msg) = {
        let db = get_read_only_db()?;
        match db.get_newest_unchecked_message(server_id)? {
            Some(msg) => (
                msg.channel,
                GetMessages::new().around(msg.id).limit(LIMIT),
                Some(msg.id),
            ),
            None => {
                // if there is nothing to query we really don't need to spam the api all the time
                if rng.random::<f64>() > 0.015 {
                    return Ok(0);
                }
                let channels = db.get_known_channels(server_id)?;
                let channel = channels
                    .choose(rng)
                    .ok_or(Error::ConstStr("No known channels to query"))?;

                (channel.id, GetMessages::new().limit(LIMIT), None)
            }
        }
    };

    let messages = ChannelId::new(channel_id).messages(ctx, query).await?;
    if messages.is_empty() {
        debug!("received no messages to process");
        return Ok(0);
    }

    let db = get_writeable_db()?;
    let len = messages.len();
    info!("received {len} messages for channel id: {channel_id} and query_string {query:?}");
    let mut ids = HashSet::with_capacity(len);
    for mut msg in messages {
        let id = msg.id.get();
        ids.insert(id);

        if msg.author.bot || !regular_text_msg(msg.kind) {
            if base_msg == Some(id) {
                warn!("base msg id {id} either a bot or not a regular text message");
                db.soft_delete_message(id)?;
            }
            continue;
        }

        let db_msg_maybe = db.get_message(id)?;
        if msg.guild_id.is_none() {
            msg.guild_id = Some(GuildId::new(server_id));
        }
        if let Err(why) = process_message(ctx, &msg, false).await {
            warn!("Failed to process old message {id} with error {why:?}");
        }

        match db_msg_maybe {
            // mark as checked old if we had this in the db before processing just now
            Some(db_msg) if !db_msg.is_deleted() && !db_msg.is_checked_old() => {
                db.mark_message_checked_old(id)?;
            }
            Some(_) => {}
            None => {
                debug!("message {id} not already in db, must have been sent whilst server down")
            }
        }
    }

    // if when querying around ID the ID itself doesn't show up,
    // this would seem to indicate it was deleted and we missed it.
    // As this is unclear, we just soft delete it.
    if let Some(base_msg) = base_msg.filter(|id| !ids.contains(id)) {
        warn!("did not received base msg id {base_msg} when querying for messages");
        db.soft_delete_message(base_msg)?;
    }

    Ok(len)
}

/// Continuously works backwards through a guild's history, processing
/// messages that were missed or need to be processed again
async fn backfill_old_messages(ctx: Context, guild: GuildId) {
    // Arbitrary seed, technically this means everything is deterministic
    // but we don't actually care for this purpose
    let mut rng = SmallRng::seed_from_u64(1337);
    loop {
        let tts = match process_old_messages(&ctx, guild.get(), &mut rng).await {
            Ok(0) => 10 * 60,
            Ok(_) => 45,
            Err(why) => {
                warn!("process old messages with err {why:?}");
                240
            }
        };
        trace!("process old message task sleeping {tts}s");
        tokio::time::sleep(Duration::from_secs(tts)).await;
    }
}

/// Brings the stored channels for a guild up to date with discord, and
/// stores the most recent message in each so old messages can be backfilled
async fn sync_guild_channels(ctx: &Context, guild: GuildId) -> Result<()> {
    let channels: HashMap<ChannelId, GuildChannel> = guild
        .channels(ctx)
        .await?
        .into_iter()
        .filter(|(_, c)| c.kind != ChannelType::Voice && c.kind != ChannelType::Category)
        .collect();

    let channel_list: Vec<&str> = channels.values().map(|c| c.name.as_str()).collect();
    info!("found server with id {guild} and channels {channel_list:?}");

    let db = get_writeable_db()?;
    match db.get_channel_list(guild.get()) {
        Ok(stored) => {
            for (id, name) in stored {
                if !channels.contains_key(&ChannelId::new(id)) {
                    warn!(
                        "stored channel {name} with id {id} no longer exists on server, deleting"
                    );
                    log_error(db.delete_channel(id), "Db delete channel");
                }
            }
        }
        Err(why) => error!("failed to load stored channels for guild {guild}: {why:?}"),
    }

    // insert all channels to update names and visibility
    let mut visible_channels = Vec::with_capacity(channels.len());
    for (id, channel) in &channels {
        let visible = bot_read_channel_permission(ctx, channel);
        log_error(
            db.update_channel(id.get(), guild.get(), &channel.name, visible),
            "Db update channel",
        );
        if visible {
            visible_channels.push(*id);
        }
    }

    // store the most recent message in each channel
    let latest_messages = join_all(
        visible_channels
            .iter()
            .map(|id| ctx.http.get_messages(*id, None, Some(1))),
    )
    .await;
    for (id, result) in visible_channels.iter().zip(latest_messages) {
        match result {
            Ok(mut msg_vec) => {
                if let Some(msg) = msg_vec.pop().filter(|msg| !msg.author.bot) {
                    log_error(
                        db.add_message(
                            msg.id.get(),
                            msg.channel_id.get(),
                            guild.get(),
                            msg.author.id.get(),
                        ),
                        "db add message",
                    );
                }
            }
            Err(why) => warn!("failed to load most recent message for id {id} {why:?}"),
        }
    }
    Ok(())
}

fn store_member(member: &Member) -> Result<()> {
    let db = get_writeable_db()?;
    let user_id = member.user.id.get();
    db.add_user(user_id, &member.user.name, member.user.bot)?;
    if let Some(nickname) = &member.nick {
        db.add_nickname(user_id, member.guild_id.get(), nickname)?;
    }
    Ok(())
}

impl Handler {
    /// Starts backfilling old messages for a guild, unless already started
    fn spawn_backfill(&self, ctx: &Context, guild: GuildId) {
        let newly_added = self
            .backfill_guilds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(guild);
        if newly_added {
            tokio::spawn(backfill_old_messages(ctx.clone(), guild));
        } else {
            debug!("backfill already running for guild {guild}");
        }
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
            Err(Error::BotMessage) => debug!("Skipped processing bot message"),
            Err(why) => error!("message: failed to process messsage: {why:?}"),
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
        match process_message_update(&event).await {
            Ok(Some(reply)) => {
                if let Err(why) = reply.send(&ctx).await {
                    error!("message_update: failed to send reply {why:?}");
                }
            }
            Ok(None) => {}
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
        match writable_db_call(|db| db.delete_message(message_id.get())) {
            Ok(()) => info!("successfully deleted message id {message_id} from db"),
            Err(why) => {
                error!("failed to delete message id {message_id} with following error {why:?}");
            }
        }
    }

    async fn channel_create(&self, ctx: Context, channel: GuildChannel) {
        let visible = bot_read_channel_permission(&ctx, &channel);
        log_error(
            writable_db_call(|db| {
                db.update_channel(
                    channel.id.get(),
                    channel.guild_id.get(),
                    &channel.name,
                    visible,
                )
            }),
            "Db update channel",
        );
    }

    async fn channel_update(
        &self,
        ctx: Context,
        _old: Option<GuildChannel>,
        channel: GuildChannel,
    ) {
        let visible = bot_read_channel_permission(&ctx, &channel);
        info!(
            "received channel update for channel id {} with name {} in server {}, visibility is now: {visible}",
            channel.id, channel.name, channel.guild_id
        );
        log_error(
            writable_db_call(|db| db.update_channel_visibility(channel.id.get(), visible)),
            "Updating visibility",
        );
    }

    async fn channel_delete(
        &self,
        _ctx: Context,
        channel: GuildChannel,
        _messages: Option<Vec<Message>>,
    ) {
        trace!("recieved channel delete for {channel:?}");
        log_error(
            writable_db_call(|db| db.delete_channel(channel.id.get())),
            "Db delete channel",
        );
    }

    async fn guild_member_update(
        &self,
        _ctx: Context,
        _old_if_available: Option<Member>,
        new: Option<Member>,
        _event: GuildMemberUpdateEvent,
    ) {
        if let Some(member) = new {
            log_error(store_member(&member), "Storing updated member");
        }
    }

    async fn ready(&self, _: Context, ready: Ready) {
        info!("{} is connected!", ready.user.name);
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        for guild in guilds {
            log_error(
                writable_db_call(|db| db.update_server(guild.get(), guild.name(&ctx).as_deref())),
                "Update server name from cache_ready",
            );

            self.spawn_backfill(&ctx, guild);

            if let Err(why) = sync_guild_channels(&ctx, guild).await {
                error!("failed to sync channels for guild {guild} with error {why:?}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_regular_text_msg() {
        assert!(regular_text_msg(MessageType::Regular));
        assert!(regular_text_msg(MessageType::InlineReply));
        assert!(!regular_text_msg(MessageType::PinsAdd));
        assert!(!regular_text_msg(MessageType::MemberJoin));
    }
}
