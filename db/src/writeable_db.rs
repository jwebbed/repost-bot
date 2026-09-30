use crate::ReadOnlyDb;
use crate::connections::GetConnectionMutable;
use crate::queries::{self, hash_samples};
use crate::structs::Message;

use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use rusqlite::{Error, Result};

pub trait WriteableDb: GetConnectionMutable + ReadOnlyDb {
    #[inline]
    fn update_server(&self, server_id: u64, name: Option<&str>) -> Result<()> {
        let count = self
            .get_connection()
            .prepare_cached(
                "INSERT INTO server (id, name) VALUES ( ?1, ?2 )
                ON CONFLICT(id) DO UPDATE SET name=excluded.name
                WHERE (server.name IS NULL AND excluded.name IS NOT NULL)",
            )?
            .execute((server_id, name))?;

        if count > 0 {
            info!(
                "Added server_id {server_id} with name {} to db",
                name.unwrap_or("NULL")
            );
        }

        Ok(())
    }

    #[inline]
    fn add_message(
        &self,
        message_id: u64,
        channel_id: u64,
        server_id: u64,
        author_id: u64,
    ) -> Result<Message> {
        let conn = self.get_connection();
        conn.prepare_cached(
            "INSERT INTO message (id, server, channel, created_at, author)
            VALUES ( ?1, ?2, ?3, ?4, ?5 )
            ON CONFLICT(id) DO UPDATE SET author=excluded.author
            WHERE (message.author IS NULL)",
        )?
        .execute((
            message_id,
            server_id,
            channel_id,
            get_snowflake_creation_time(message_id),
            author_id,
        ))?;

        queries::get_message(conn, message_id)?.ok_or_else(|| {
            // should return a special error at some point
            warn!("No message with input id found despite being just added");
            Error::QueryReturnedNoRows
        })
    }

    #[inline]
    fn add_user(&self, user_id: u64, username: &str, bot: bool) -> Result<()> {
        self.get_connection()
            .prepare_cached(
                "INSERT INTO user (id, username, bot)
                VALUES ( ?1, ?2, ?3 )
                ON CONFLICT(id) DO UPDATE SET
                    username=excluded.username,
                    bot=excluded.bot
                WHERE (
                    user.username != excluded.username OR
                    user.bot != excluded.bot
                )",
            )?
            .execute((user_id, username, bot))?;
        Ok(())
    }

    #[inline]
    fn add_nickname(&self, user_id: u64, server_id: u64, nickname: &str) -> Result<()> {
        self.get_connection()
            .prepare_cached(
                "INSERT OR IGNORE INTO nickname (user, server, nickname)
                VALUES ( ?1, ?2, ?3 )",
            )?
            .execute((user_id, server_id, nickname))?;
        Ok(())
    }

    #[inline]
    fn mark_message_all_checked(&self, message_id: u64) -> Result<()> {
        // will probably want to break this back up to seperate functions
        // at some point just not important right now
        self.execute(
            "UPDATE message
            SET
                parsed_repost=datetime('now'),
                parsed_embed=datetime('now')
            WHERE id=(?1)",
            [message_id],
        )
    }

    #[inline]
    fn mark_message_checked_old(&self, message_id: u64) -> Result<()> {
        self.execute(
            "UPDATE message
            SET checked_old=datetime('now')
            WHERE id=(?1)",
            [message_id],
        )
    }

    #[inline]
    fn delete_message(&self, message_id: u64) -> Result<()> {
        self.execute("DELETE FROM message WHERE id=(?1)", [message_id])
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
        let count = self
            .get_connection()
            .prepare_cached(
                "INSERT INTO channel (id, name, server, visible) VALUES ( ?1, ?2, ?3, ?4 )
                ON CONFLICT(id) DO UPDATE SET
                    name=excluded.name,
                    visible=excluded.visible
                WHERE (
                    channel.name != excluded.name OR
                    channel.visible != excluded.visible
                )",
            )?
            .execute((channel_id, name, server_id, visible))?;

        if count > 0 {
            debug!("Added/updated channel_id {channel_id} with name {name} to db");
        }
        Ok(())
    }

    #[inline]
    fn update_channel_visibility(&self, channel_id: u64, visible: bool) -> Result<()> {
        self.execute(
            "UPDATE channel SET visible = (?1) WHERE id = (?2)",
            (visible, channel_id),
        )
    }

    #[inline]
    fn delete_channel(&self, channel_id: u64) -> Result<()> {
        self.execute("DELETE FROM channel WHERE id = (?1)", [channel_id])
    }

    /// Stores every link as having been posted in `message_id` in a single
    /// transaction. A link is only associated with a message once.
    fn insert_links<'a, I>(&mut self, links: I, message_id: u64) -> Result<()>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let tx = self.get_mutable_connection().transaction()?;
        {
            let mut insert_link = tx.prepare_cached(
                "INSERT INTO link (link) VALUES (?1) ON CONFLICT(link) DO NOTHING;",
            )?;
            let mut insert_message_link = tx.prepare_cached(
                "INSERT INTO message_link (link, message)
                SELECT L.id, ?2 FROM link AS L
                WHERE L.link=(?1) AND NOT EXISTS (
                    SELECT 1 FROM message_link AS ML
                    WHERE ML.link=L.id AND ML.message=?2
                );",
            )?;
            for link in links {
                debug!("Inserting the following link {link:?}");
                insert_link.execute([link])?;
                insert_message_link.execute((link, message_id))?;
            }
        }
        tx.commit()
    }

    /// Stores every `(url, hash)` image as having been posted in `message_id`
    /// in a single transaction. An image is only associated with a message once.
    fn insert_images<'a, I>(&mut self, images: I, message_id: u64) -> Result<()>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let tx = self.get_mutable_connection().transaction()?;
        {
            let mut insert_image = tx.prepare_cached(
                "INSERT INTO image (c1, c2, c3, c4, c5, hash, url)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                ON CONFLICT(url) DO NOTHING;",
            )?;
            let mut insert_message_image = tx.prepare_cached(
                "INSERT INTO message_image (image, message)
                SELECT I.id, ?2 FROM image AS I
                WHERE I.url=(?1) AND NOT EXISTS (
                    SELECT 1 FROM message_image AS MI
                    WHERE MI.image=I.id AND MI.message=?2
                );",
            )?;
            for (url, hash) in images {
                debug!("Inserting the following image hash {hash:?}");
                let [c1, c2, c3, c4, c5] = hash_samples(hash)?;
                insert_image.execute((c1, c2, c3, c4, c5, hash, url))?;
                insert_message_image.execute((url, message_id))?;
            }
        }
        tx.commit()
    }

    #[inline]
    fn add_reply(&self, message_id: u64, channel_id: u64, replied_id: u64) -> Result<()> {
        self.get_connection()
            .prepare_cached(
                "INSERT INTO reply (id, channel, replied_to)
                VALUES ( ?1, ?2, ?3 )
                ON CONFLICT(id) DO NOTHING",
            )?
            .execute([message_id, channel_id, replied_id])?;
        Ok(())
    }
}

/// Discord's epoch starts at "2015-01-01T00:00:00+00:00"
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

#[inline]
pub(crate) fn get_snowflake_creation_time(snowflake: u64) -> DateTime<Utc> {
    // (u64::MAX >> 22) + DISCORD_EPOCH comfortably fits in an i64
    DateTime::from_timestamp_millis(((snowflake >> 22) + DISCORD_EPOCH) as i64)
        .expect("somehow received an invalid snowflake")
}
