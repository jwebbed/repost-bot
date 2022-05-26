mod migrations;
mod queries;
pub mod structs;

use crate::structs::wordle::{LetterStatus, Wordle, WordleBoard};
use crate::structs::{Channel, Link, Message, Reply, RepostCount, ReposterCount};

use log::{debug, info, warn};
use rusqlite::types::ToSql;

use rusqlite::{params, Connection, Error, OptionalExtension, Result, Row};
use serenity::model::id::{ChannelId, GuildId, MessageId};
use std::cell::RefCell;

pub struct DB {
    conn: RefCell<Connection>,
}

impl DB {
    #[inline(always)]
    fn get_connection() -> Result<Connection> {
        const DB_FILE_PATH: &str = "./repost.db3";
        Connection::open(DB_FILE_PATH)
    }

    fn get_test_db() -> Result<DB> {
        Ok(DB {
            conn: RefCell::new(Connection::open_in_memory()?),
        })
    }

    pub fn get_db() -> Result<DB> {
        Ok(DB {
            conn: RefCell::new(DB::get_connection()?),
        })
    }

    pub fn db_call<F, T>(f: F) -> Result<T>
    where
        F: FnOnce(DB) -> Result<T>,
    {
        f(DB::get_db()?)
    }

    pub fn migrate() -> Result<()> {
        migrations::migrate(&mut DB::get_connection()?)
    }

    pub fn update_server(&self, server_id: u64, name: &Option<String>) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare_cached(
            "INSERT INTO server (id, name) VALUES ( ?1, ?2 )
            ON CONFLICT(id) DO UPDATE SET name=excluded.name
            WHERE (server.name IS NULL AND excluded.name IS NOT NULL)",
        )?;

        let count = match name {
            Some(n) => stmt.execute(params![server_id, n]),
            None => stmt.execute(params![server_id, rusqlite::types::Null]),
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

        /*
        match match name {
            Some(n) => stmt.execute(params![server_id, n]),
            None => stmt.execute(params![server_id, rusqlite::types::Null]),
        } {
            Ok(cnt) => {
                if cnt > 0 {
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
            Err(why) => Err(why),
        }*/
    }
    pub fn get_message(&self, message_id: MessageId) -> Result<Option<Message>> {
        let conn = self.conn.borrow();
        queries::get_message(&conn, *message_id.as_u64())
    }

    pub fn get_newest_unchecked_message(&self, server_id: u64) -> Result<Option<Message>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT M.id, M.server, M.channel, M.author, M.created_at, 
                    M.parsed_repost, M.parsed_wordle, M.deleted, M.checked_old, 
                    M.parsed_embed
            FROM message as M 
            JOIN channel as C on C.id=M.channel
            WHERE 
                C.visible=TRUE AND
                M.deleted IS NULL AND
                M.server=(?1) AND 
                ( M.checked_old IS NULL OR
                  M.parsed_embed IS NULL )
            ORDER BY M.created_at desc
            LIMIT 1",
        )?;
        let mut rows = stmt.query_map([server_id], |row| {
            Ok(Message::new(
                row.get(0)?, // id
                row.get(1)?, // server
                row.get(2)?, // channel
                row.get(3)?, // author
                row.get(4)?, // created_at
                row.get(5)?, // parsed_repost
                row.get(6)?, // parsed_wordle
                row.get(9)?, // parsed_embed
                row.get(7)?, // deleted
                row.get(8)?, // checked_old
            ))
        })?;
        extract_first_result(&mut rows)
    }

    pub fn get_known_channels(&self, server_id: u64) -> Result<Vec<Channel>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT * FROM channel 
            WHERE server=(?1) AND 
                visible=TRUE",
        )?;

        let rows = stmt.query_map([server_id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                name: row.get(1)?,
                visible: row.get(2)?,
                server: row.get(3)?,
            })
        })?;

        let mut channels = Vec::new();
        for row in rows {
            channels.push(row?)
        }
        Ok(channels)
    }

    pub fn add_message(
        &self,
        message_id: MessageId,
        channel_id: u64,
        server_id: u64,
        author_id: u64,
    ) -> Result<Message> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "INSERT INTO message (id, server, channel, created_at, author) 
            VALUES ( ?1, ?2, ?3, ?4, ?5 )
            ON CONFLICT(id) DO UPDATE SET author=excluded.author
            WHERE (message.author IS NULL)",
        )?;

        let msg_id64 = *message_id.as_u64();
        stmt.execute(params![
            msg_id64,
            server_id,
            channel_id,
            *message_id.created_at(),
            author_id
        ])?;

        match queries::get_message(&conn, msg_id64)? {
            Some(msg) => Ok(msg),
            None => {
                // should return a special error at some point
                warn!("No message with input id found despite being just added");
                Err(Error::QueryReturnedNoRows)
            }
        }
    }

    pub fn add_user(
        &self,
        user_id: u64,
        username: &str,
        bot: bool,
        discriminator: u16,
    ) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
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

        stmt.execute(params![user_id, username, bot, discriminator])?;

        Ok(())
    }

    pub fn add_nickname(&self, user_id: u64, server_id: u64, nickname: &str) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "INSERT OR IGNORE INTO nickname (user, server, nickname) 
            VALUES ( ?1, ?2, ?3 )",
        )?;

        stmt.execute(params![user_id, server_id, nickname])?;

        Ok(())
    }

    pub fn mark_message_all_checked(&self, message_id: MessageId) -> Result<()> {
        // will probably want to break this back up to seperate functions
        // at some point just not important right now
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE message 
            SET 
                parsed_repost=datetime('now'), 
                parsed_wordle=datetime('now'),
                parsed_embed=datetime('now')
            WHERE id=(?1)",
            [*message_id.as_u64()],
        )?;
        Ok(())
    }

    pub fn mark_message_checked_old(&self, message_id: MessageId) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE message 
            SET checked_old=datetime('now')
            WHERE id=(?1)",
            [*message_id.as_u64()],
        )?;
        Ok(())
    }

    pub fn delete_message(&self, message_id: MessageId) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute("DELETE FROM message WHERE id=(?1)", [*message_id.as_u64()])?;
        Ok(())
    }

    // Soft delete is for when we query for a message, but get no result,
    // this can occur if a message was deleted whilst the bot was down.
    //
    // If we see a message deleted we should (and do) just delete the message
    // outright from the DB. Soft delete is for this other case as we aren't
    // entirely sure what we should do with this. For safety not deleting and
    // and just filtering from relevent queries.
    pub fn soft_delete_message(&self, message_id: u64) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE message SET deleted=datetime('now') WHERE id=(?1)",
            [message_id],
        )?;
        Ok(())
    }

    pub fn update_channel(
        &self,
        channel_id: u64,
        server_id: u64,
        name: &str,
        visible: bool,
    ) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "INSERT INTO channel (id, name, server, visible) VALUES ( ?1, ?2, ?3, ?4 )
            ON CONFLICT(id) DO UPDATE SET 
                name=excluded.name,
                visible=excluded.visible
            WHERE (
                channel.name != excluded.name OR
                channel.visible != excluded.visible
            )",
        )?;

        match stmt.execute(params![channel_id, name, server_id, visible]) {
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

    pub fn update_channel_visibility(&self, channel_id: ChannelId, visible: bool) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE channel SET visible = (?1) WHERE id = (?2)",
            params![visible, *channel_id.as_u64()],
        )?;
        Ok(())
    }

    pub fn delete_channel(&self, channel_id: ChannelId) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "DELETE FROM channel WHERE id = (?1)",
            params![*channel_id.as_u64()],
        )?;
        Ok(())
    }

    pub fn get_channel_list(&self, server_id: GuildId) -> Result<Vec<(ChannelId, String)>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare("SELECT id, name FROM channel where server = (?1)")?;
        let rows = stmt.query_map(params![*server_id.as_u64()], |row| {
            Ok((ChannelId(row.get(0)?), row.get(1)?))
        })?;
        let mut links = Vec::new();
        for row in rows {
            links.push(row?)
        }
        Ok(links)
    }

    pub fn insert_link(&self, link: &str, message_id: u64) -> Result<()> {
        debug!("Inserting the following link {:?}", link);

        let mut conn = self.conn.borrow_mut();
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
            params![link, message_id],
        )?;

        tx.commit()?;

        Ok(())
    }

    pub fn insert_image(&self, url: &str, hash: &str, message_id: u64) -> Result<()> {
        debug!("Inserting the following image hash {:?}", hash);

        let mut conn = self.conn.borrow_mut();
        let tx = conn.transaction()?;

        let mut chars = hash.chars();

        tx.execute(
            "INSERT INTO image (c1, c2, c3, c4, c5, hash, url) 
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(url) DO NOTHING;",
            params![
                String::from(chars.next().unwrap()),
                String::from(chars.nth(1).unwrap()),
                String::from(chars.nth(2).unwrap()),
                String::from(chars.nth(3).unwrap()),
                String::from(chars.nth(4).unwrap()),
                hash,
                url
            ],
        )?;

        tx.execute(
            "INSERT INTO message_image (image, message)
            VALUES (
                (SELECT id FROM image WHERE url=(?1)),
                ?2
            );",
            params![url, message_id],
        )?;

        tx.commit()?;

        Ok(())
    }

    pub fn query_links(&self, link: &str, server: u64) -> Result<Vec<Link>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT 
                L.id, L.link, S.id, C.id, M.id, M.created_at, C.name, 
                S.name, M.author, M.parsed_repost, M.parsed_wordle, 
                M.deleted, M.checked_old, M.parsed_embed
            FROM link AS L 
            JOIN message_link as ML on ML.link=L.id
            JOIN message as M on ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            WHERE 
                L.link = (?1)
                AND S.id = (?2)
                AND C.visible = TRUE
                AND M.deleted IS NULL;",
        )?;
        let rows = stmt.query_map(params![link, server], |row| {
            Ok(Link {
                id: row.get(0)?,
                link: row.get(1)?,
                channel_name: row.get(6)?,
                server_name: row.get(7)?,
                message: Message::new(
                    row.get(4)?,  // id
                    row.get(2)?,  // server
                    row.get(3)?,  // channel
                    row.get(8)?,  // author
                    row.get(5)?,  // created_at
                    row.get(9)?,  // parsed_repost
                    row.get(10)?, // parsed_wordle
                    row.get(13)?, // parsed_embed
                    row.get(11)?, // deleted
                    row.get(12)?, // checked_old
                ),
            })
        })?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row?)
        }

        Ok(links)
    }

    pub fn query_reposts_for_message(&self, message_id: u64) -> Result<Vec<Message>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT 
                MR.id, MR.server, MR.channel, MR.author, MR.created_at, MR.parsed_repost, 
                MR.parsed_wordle, MR.deleted, MR.checked_old, MR.parsed_embed
            FROM message_link AS ML
            JOIN message AS M ON ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            JOIN message_link AS MLR ON ML.link=MLR.link
            JOIN message AS MR ON MLR.message = MR.id
            WHERE 
                M.id = (?1)
                AND C.visible = TRUE
                AND M.deleted IS NULL
                AND M.server == MR.server
                AND ML.id != MLR.id
                AND MR.created_at < M.created_at",
        )?;
        let rows = stmt.query_map(params![message_id], |row| {
            Ok(Message::new(
                row.get(0)?, // id
                row.get(1)?, // server
                row.get(2)?, // channel
                row.get(3)?, // author
                row.get(4)?, // created_at
                row.get(5)?, // parsed_repost
                row.get(6)?, // parsed_wordle
                row.get(9)?, // parsed_embed
                row.get(7)?, // deleted
                row.get(8)?, // checked_old
            ))
        })?;
        let mut posts = Vec::new();
        for row in rows {
            posts.push(row?);
        }
        Ok(posts)
    }

    pub fn hash_matches(
        &self,
        hash: &str,
        server: u64,
        current_msg_id: u64,
    ) -> Result<Vec<(Message, String)>> {
        let conn = self.conn.borrow();

        let mut stmt = conn.prepare(
            "SELECT M.id, M.server, M.channel, M.author, M.created_at, 
            M.parsed_repost, M.parsed_wordle, M.deleted, M.checked_old, 
            M.parsed_embed, I.hash
            FROM image as I
            JOIN message_image as MI on MI.image=I.id
            JOIN message as M on M.id=MI.message
            JOIN server AS S ON M.server=S.id
            JOIN channel AS C ON M.channel=C.id
            WHERE 
            (   I.hash = (?6) OR
                ( I.c1 = (?1) AND I.c2 = (?2) AND I.c3 = (?3) AND I.c4 = (?4) ) OR
                ( I.c2 = (?2) AND I.c3 = (?3) AND I.c4 = (?4) AND I.c5 = (?5) ) OR
                ( I.c3 = (?3) AND I.c4 = (?4) AND I.c5 = (?5) AND I.c1 = (?1) ) OR
                ( I.c4 = (?4) AND I.c5 = (?5) AND I.c1 = (?1) AND I.c2 = (?2) ) OR
                ( I.c5 = (?5) AND I.c1 = (?1) AND I.c2 = (?2) AND I.c3 = (?3) )
            )
            AND S.id = (?7)
            AND M.id != (?8)
            AND C.visible = TRUE
            AND M.deleted IS NULL",
        )?;
        let mut chars = hash.chars();
        let rows = stmt.query_map(
            params![
                String::from(chars.next().unwrap()),
                String::from(chars.nth(1).unwrap()),
                String::from(chars.nth(2).unwrap()),
                String::from(chars.nth(3).unwrap()),
                String::from(chars.nth(4).unwrap()),
                hash,
                server,
                current_msg_id
            ],
            |row: &Row<'_>| -> rusqlite::Result<(Message, String)> {
                Ok((
                    Message::new(
                        row.get(0)?, // id
                        row.get(1)?, // server
                        row.get(2)?, // channel
                        row.get(3)?, // author
                        row.get(4)?, // created_at
                        row.get(5)?, // parsed_repost
                        row.get(6)?, // parsed_wordle
                        row.get(9)?, // parsed_embed
                        row.get(7)?, // deleted
                        row.get(8)?, // checked_old
                    ),
                    row.get(10)?,
                ))
            },
        )?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row?)
        }

        Ok(links)
    }

    pub fn get_repost_list(&self, server_id: u64) -> Result<Vec<RepostCount>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT L.link, LM.link_count 
            FROM link as L JOIN (
                SELECT 
                    ML.link, 
                    COUNT(1) as link_count,
                    MAX(M.created_at) as most_recent 
                FROM message_link as ML 
                JOIN message as M on ML.message=M.id
                JOIN channel as C on M.channel=C.id
                WHERE M.server=(?1) AND 
                    C.visible=TRUE AND
                    M.deleted IS NULL
                GROUP BY link
                HAVING link_count > 1 
            ) as LM on L.id=LM.link
            ORDER BY link_count desc, most_recent desc
            LIMIT 10",
        )?;

        let rows = stmt.query_map(params![server_id], |row| {
            Ok(RepostCount {
                link: row.get(0)?,
                count: row.get(1)?,
            })
        })?;

        let mut reposts = Vec::new();
        for repost in rows {
            reposts.push(repost?)
        }

        Ok(reposts)
    }

    pub fn get_top_reposters(&self, server_id: u64) -> Result<Vec<ReposterCount>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT U.username, COUNT(*) as cnt
            FROM message_link AS L1
            JOIN (
                SELECT MIN(ML.id) as id, ML.link
                FROM message_link as ML
                JOIN message as M on M.id = ML.message 
                JOIN channel as C on M.channel=C.id
                WHERE M.server=(?1) AND 
                    C.visible=TRUE AND
                    M.deleted IS NULL
                GROUP BY link
            ) as L2 on L1.link=L2.link
            JOIN message as M on M.id=L1.message
            JOIN user as U on M.author=U.id
            WHERE L1.id != L2.id
            GROUP BY U.username
            ORDER BY cnt desc",
        )?;

        let rows = stmt.query_map(params![server_id], |row| {
            Ok(ReposterCount {
                username: row.get(0)?,
                count: row.get(1)?,
            })
        })?;

        let mut reposters = Vec::new();
        for repost in rows {
            reposters.push(repost?)
        }

        Ok(reposters)
    }

    pub fn insert_wordle(&self, message_id: u64, wordle: &Wordle) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(&format!(
            "INSERT INTO wordle VALUES ({})
            ON CONFLICT(message) DO NOTHING",
            repeat_vars(4 + 5 * 6)
        ))?;
        /*
        match stmt.execute(rusqlite::params_from_iter(
            wordle.get_query_parts(message_id),
        )) {
            Ok(_) => Ok(()),
            Err(why) => Err(Error::from(why)),
        }*/

        stmt.execute(rusqlite::params_from_iter(
            wordle.get_query_parts(message_id),
        ))?;

        Ok(())
    }
    pub fn get_wordles_for_author(&self, author_id: u64, server_id: u64) -> Result<Vec<Wordle>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT W.*, MIN(M.created_at)
            FROM wordle as W 
            JOIN message as M on W.message=M.id
            WHERE M.author=(?1) AND 
                M.server=(?2) AND
                M.deleted IS NULL
            GROUP BY W.number",
        )?;

        let rows = stmt.query_map([author_id, server_id], |row| {
            let mut board: [[LetterStatus; 5]; 6] = Default::default();
            for i in 0..30 {
                board[i / 5][i % 5] = row.get(i + 4)?;
            }
            Ok(Wordle {
                number: row.get(1)?,
                score: row.get(2)?,
                hardmode: row.get(3)?,
                board: WordleBoard(board),
            })
        })?;

        let mut wordles = Vec::new();
        for wordle in rows {
            wordles.push(wordle?)
        }

        Ok(wordles)
    }

    pub fn get_wordles_for_server(&self, server_id: u64) -> Result<Vec<Wordle>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT W.*, MIN(M.created_at)
            FROM wordle as W 
            JOIN message as M on W.message=M.id
            WHERE M.server=(?1) AND M.deleted IS NULL
            GROUP BY M.author, W.number",
        )?;

        let rows = stmt.query_map([server_id], |row| {
            let mut board: [[LetterStatus; 5]; 6] = Default::default();
            for i in 0..30 {
                board[i / 5][i % 5] = row.get(i + 4)?;
            }
            Ok(Wordle {
                number: row.get(1)?,
                score: row.get(2)?,
                hardmode: row.get(3)?,
                board: WordleBoard(board),
            })
        })?;

        let mut wordles = Vec::new();
        for wordle in rows {
            wordles.push(wordle?)
        }

        Ok(wordles)
    }

    pub fn add_reply(&self, message_id: u64, channel_id: u64, replied_id: u64) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "INSERT INTO reply (id, channel, replied_to) 
            VALUES ( ?1, ?2, ?3 )
            ON CONFLICT(id) DO NOTHING",
        )?;

        stmt.execute(params![message_id, channel_id, replied_id])?;
        Ok(())
    }

    pub fn get_reply(&self, replied_id: u64) -> Result<Option<Reply>> {
        let conn = self.conn.borrow();
        conn.query_row(
            "SELECT  id, channel, replied_to
            FROM reply WHERE replied_to=(?1)",
            [replied_id],
            |row| {
                Ok(Reply {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    replied_to: row.get(2)?,
                })
            },
        )
        .optional()
    }
}

impl Wordle {
    #[inline(always)]
    fn get_query_parts(&self, message_id: u64) -> Vec<Box<dyn ToSql>> {
        let mut ret: Vec<Box<dyn ToSql>> = vec![
            Box::new(message_id),
            Box::new(self.number),
            Box::new(self.score),
            Box::new(self.hardmode),
        ];

        for element in self.board {
            ret.push(Box::new(element));
        }
        ret
    }
}

#[inline(always)]
fn repeat_vars(count: usize) -> String {
    assert_ne!(count, 0);
    let mut s = "?,".repeat(count);
    // Remove trailing comma
    s.pop();
    s
}

#[inline(always)]
fn extract_first_result<I, T>(iter: &mut I) -> Result<Option<T>>
where
    I: Iterator<Item = rusqlite::Result<T>>,
{
    // I hate this less than what I was doing before, but still works
    let ret = match iter.next() {
        Some(ret) => Some(ret?),
        None => None,
    };

    Ok(ret)
}
