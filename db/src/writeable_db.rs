use crate::connections::GetConnectionMutable;
use crate::queries;
use crate::structs::Message;
use crate::ReadOnlyDb;

use log::{debug, info, warn};
use rusqlite::{Error, Result};
use serenity::model::id::{ChannelId, MessageId};

pub trait WriteableDb: GetConnectionMutable + ReadOnlyDb {
    #[inline]
    fn update_server(&self, server_id: u64, name: &Option<String>) -> Result<()> {
        let mut stmt = self.get_connection().prepare(
            "INSERT INTO server (id, name) VALUES ( ?1, ?2 )
            ON CONFLICT(id) DO UPDATE SET name=excluded.name
            WHERE (server.name IS NULL AND excluded.name IS NOT NULL)",
        )?;

        let count = match name {
            Some(n) => stmt.execute((server_id, n)),
            None => stmt.execute((server_id, rusqlite::types::Null)),
        }?;

        if count > 0 {
            info!(
                "Added server_id {} with name {} to db",
                server_id,
                match name {
                    Some(n) => n,
                    None => "NULL",
                }
            );
        };

        Ok(())
    }

    #[inline]
    fn add_message(
        &self,
        message_id: MessageId,
        channel_id: u64,
        server_id: u64,
        author_id: u64,
    ) -> Result<Message> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare(
            "INSERT INTO message (id, server, channel, created_at, author) 
            VALUES ( ?1, ?2, ?3, ?4, ?5 )
            ON CONFLICT(id) DO UPDATE SET author=excluded.author
            WHERE (message.author IS NULL)",
        )?;

        let msg_id64 = *message_id.as_u64();
        stmt.execute((
            msg_id64,
            server_id,
            channel_id,
            *message_id.created_at(),
            author_id,
        ))?;

        match queries::get_message(conn, msg_id64)? {
            Some(msg) => Ok(msg),
            None => {
                // should return a special error at some point
                warn!("No message with input id found despite being just added");
                Err(Error::QueryReturnedNoRows)
            }
        }
    }

    #[inline]
    fn add_user(&self, user_id: u64, username: &str, bot: bool, discriminator: u16) -> Result<()> {
        let mut stmt = self.get_connection().prepare(
            "INSERT INTO user (id, username, bot, discriminator) 
            VALUES ( ?1, ?2, ?3, ?4 )
            ON CONFLICT(id) DO UPDATE SET 
                username=excluded.username,
                bot=excluded.bot,
                discriminator=excluded.discriminator
            WHERE (
                user.username != excluded.username OR
                user.bot != excluded.bot OR
                user.discriminator != excluded.discriminator
            )",
        )?;

        stmt.execute((user_id, username, bot, discriminator))?;

        Ok(())
    }

    #[inline]
    fn add_nickname(&self, user_id: u64, server_id: u64, nickname: &str) -> Result<()> {
        let mut stmt = self.get_connection().prepare(
            "INSERT OR IGNORE INTO nickname (user, server, nickname) 
            VALUES ( ?1, ?2, ?3 )",
        )?;

        stmt.execute((user_id, server_id, nickname))?;

        Ok(())
    }

    #[inline]
    fn mark_message_all_checked(&self, message_id: MessageId) -> Result<()> {
        // will probably want to break this back up to seperate functions
        // at some point just not important right now
        self.execute(
            "UPDATE message 
            SET 
                parsed_repost=datetime('now'), 
                parsed_embed=datetime('now')
            WHERE id=(?1)",
            [*message_id.as_u64()],
        )
    }

    #[inline]
    fn mark_message_checked_old(&self, message_id: MessageId) -> Result<()> {
        self.execute(
            "UPDATE message 
            SET checked_old=datetime('now')
            WHERE id=(?1)",
            [*message_id.as_u64()],
        )
    }

    #[inline]
    fn delete_message(&self, message_id: MessageId) -> Result<()> {
        self.execute("DELETE FROM message WHERE id=(?1)", [*message_id.as_u64()])
    }

    // Soft delete is for when we query for a message, but get no result,
    // this can occur if a message was deleted whilst the bot was down.
    //
    // If we see a message deleted we should (and do) just delete the message
    // outright from the DB. Soft delete is for this other case as we aren't
    // entirely sure what we should do with this. For safety not deleting and
    // and just filtering from relevent queries.
    #[inline]
    fn soft_delete_message(&self, message_id: u64) -> Result<()> {
        self.execute(
            "UPDATE message SET deleted=datetime('now') WHERE id=(?1)",
            [message_id],
        )
    }

    #[inline]
    fn update_channel(
        &self,
        channel_id: u64,
        server_id: u64,
        name: &str,
        visible: bool,
    ) -> Result<()> {
        let mut stmt = self.get_connection().prepare(
            "INSERT INTO channel (id, name, server, visible) VALUES ( ?1, ?2, ?3, ?4 )
            ON CONFLICT(id) DO UPDATE SET 
                name=excluded.name,
                visible=excluded.visible
            WHERE (
                channel.name != excluded.name OR
                channel.visible != excluded.visible
            )",
        )?;

        match stmt.execute((channel_id, name, server_id, visible)) {
            Ok(cnt) => {
                if cnt > 0 {
                    debug!(
                        "Added/updated channel_id {} with name {} to db",
                        channel_id, name
                    );
                };

                Ok(())
            }
            Err(why) => Err(why),
        }
    }

    #[inline]
    fn update_channel_visibility(&self, channel_id: ChannelId, visible: bool) -> Result<()> {
        self.execute(
            "UPDATE channel SET visible = (?1) WHERE id = (?2)",
            (visible, *channel_id.as_u64()),
        )
    }

    #[inline]
    fn delete_channel(&self, channel_id: ChannelId) -> Result<()> {
        self.execute(
            "DELETE FROM channel WHERE id = (?1)",
            [*channel_id.as_u64()],
        )
    }

    #[inline]
    fn insert_link(&mut self, link: &str, message_id: u64) -> Result<()> {
        debug!("Inserting the following link {:?}", link);

        let conn = self.get_mutable_connection();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO link (link) VALUES (?1) ON CONFLICT(link) DO NOTHING;",
            [link],
        )?;
        tx.execute(
            "INSERT INTO message_link (link, message) 
            VALUES (
                (SELECT id FROM link WHERE link=(?1)), 
                ?2
            );",
            (link, message_id),
        )?;

        tx.commit()?;

        Ok(())
    }

    #[inline]
    fn insert_image(&mut self, url: &str, hash: &str, message_id: u64) -> Result<()> {
        debug!("Inserting the following image hash {:?}", hash);

        let tx = self.get_mutable_connection().transaction()?;
        let mut chars = hash.chars();

        tx.execute(
            "INSERT INTO image (c1, c2, c3, c4, c5, hash, url) 
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(url) DO NOTHING;",
            (
                String::from(chars.next().unwrap()),
                String::from(chars.nth(1).unwrap()),
                String::from(chars.nth(2).unwrap()),
                String::from(chars.nth(3).unwrap()),
                String::from(chars.nth(4).unwrap()),
                hash,
                url,
            ),
        )?;

        tx.execute(
            "INSERT INTO message_image (image, message)
            VALUES (
                (SELECT id FROM image WHERE url=(?1)),
                ?2
            );",
            (url, message_id),
        )?;

        tx.commit()?;

        Ok(())
    }

    #[inline]
    fn add_reply(&self, message_id: u64, channel_id: u64, replied_id: u64) -> Result<()> {
        let mut stmt = self.get_connection().prepare(
            "INSERT INTO reply (id, channel, replied_to) 
            VALUES ( ?1, ?2, ?3 )
            ON CONFLICT(id) DO NOTHING",
        )?;

        stmt.execute([message_id, channel_id, replied_id])?;
        Ok(())
    }
}
