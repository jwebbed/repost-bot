mod migrations;
mod queries;

use crate::errors::{Error, Result};
use crate::structs::wordle::{LetterStatus, Wordle, WordleBoard};
use crate::structs::{Link, Message, RepostCount};
use rusqlite::types::ToSql;
use rusqlite::{params, Connection};
use serenity::model::id::{ChannelId, GuildId, MessageId};
use std::cell::RefCell;

fn repeat_vars(count: usize) -> String {
    assert_ne!(count, 0);
    let mut s = "?,".repeat(count);
    // Remove trailing comma
    s.pop();
    s
}

pub struct DB {
    conn: RefCell<Connection>,
}

impl DB {
    fn get_connection() -> Result<Connection> {
        const IN_MEMORY_DB: bool = false;
        let db = if IN_MEMORY_DB {
            Connection::open_in_memory()?
        } else {
            let path = "./repost.db3";
            Connection::open(&path)?
        };

        Ok(db)
    }
    pub fn get_db() -> Result<DB> {
        // set to true to test without migration issues
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

        match match name {
            Some(n) => stmt.execute(params![server_id, n]),
            None => stmt.execute(params![server_id, rusqlite::types::Null]),
        } {
            Ok(cnt) => {
                if cnt > 0 {
                    println!(
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
            Err(why) => Err(Error::from(why)),
        }
    }

    pub fn get_newest_unchecked_message(&self, channel_id: u64) -> Result<Option<u64>> {
        let conn = self.conn.borrow();

        let ret = conn.query_row(
            "SELECT id FROM message 
            WHERE channel=(?1) AND (
                parsed_repost=FALSE
                OR parsed_wordle=FALSE
                OR author IS NULL
            )
            ORDER BY created_at desc
            LIMIT 1",
            [channel_id],
            |row| row.get(0),
        )?;

        Ok(ret)
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
            ON CONFLICT(id) DO NOTHING",
        )?;

        let msg_id64 = *message_id.as_u64();
        stmt.execute(params![
            msg_id64,
            server_id,
            channel_id,
            message_id.created_at(),
            author_id
        ])?;

        match queries::get_message(&conn, msg_id64) {
            Ok(ret) => Ok(ret),
            Err(why) => Err(Error::from(why)),
        }
    }

    pub fn mark_message_repost_checked(&self, message_id: MessageId) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE message SET parsed_repost=TRUE WHERE id=(?1)",
            [*message_id.as_u64()],
        )?;
        Ok(())
    }

    pub fn mark_message_wordle_checked(&self, message_id: MessageId) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "UPDATE message SET parsed_wordle=TRUE WHERE id=(?1)",
            [*message_id.as_u64()],
        )?;
        Ok(())
    }

    pub fn delete_message(&self, message_id: MessageId) -> Result<()> {
        let conn = self.conn.borrow();
        conn.execute(
            "DELETE FROM message WHERE id = (?1)",
            params![*message_id.as_u64()],
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
            ON CONFLICT(id) DO UPDATE SET name=excluded.name
            WHERE (channel.name != excluded.name)",
        )?;

        match stmt.execute(params![channel_id, name, server_id, visible]) {
            Ok(cnt) => {
                if cnt > 0 {
                    println!("Added channel_id {} with name {} to db", channel_id, name);
                };

                Ok(())
            }
            Err(why) => Err(Error::from(why)),
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
        println!("Inserting the following link {:?}", link);

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

    pub fn query_links(&self, link: &str, server: u64) -> Result<Vec<Link>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT 
                L.id, L.link, S.id, C.id, M.id, M.created_at, C.name, 
                S.name, M.author, M.parsed_repost, M.parsed_wordle
            FROM link AS L 
            JOIN message_link as ML on ML.link=L.id
            JOIN message as M on ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            WHERE 
                L.link = (?1)
                AND S.id = (?2)
                AND C.visible=TRUE;",
        )?;
        let rows = stmt.query_map(params![link, server], |row| {
            Ok(Link {
                id: row.get(0)?,
                link: row.get(1)?,
                channel_name: row.get(6)?,
                server_name: row.get(7)?,
                message: Message {
                    id: row.get(4)?,
                    server: row.get(2)?,
                    channel: row.get(3)?,
                    author: row.get(8)?,
                    created_at: row.get(5)?,
                    parsed_repost: row.get(9)?,
                    parsed_wordle: row.get(10)?,
                },
            })
        })?;

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
                WHERE M.server=(?1) AND C.visible=TRUE
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

    pub fn insert_wordle(&self, message_id: u64, wordle: &Wordle) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(&format!(
            "INSERT INTO wordle VALUES ({})
            ON CONFLICT(message) DO NOTHING",
            repeat_vars(4 + 5 * 6)
        ))?;

        match stmt.execute(rusqlite::params_from_iter(
            wordle.get_query_parts(message_id),
        )) {
            Ok(_) => Ok(()),
            Err(why) => Err(Error::from(why)),
        }
    }

    pub fn get_wordles_for_author(&self, author_id: u64, server_id: u64) -> Result<Vec<Wordle>> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "SELECT W.*, MIN(M.created_at)
            FROM wordle as W 
            JOIN message as M on W.message=M.id
            WHERE M.author=(?1) AND M.server=(?2)
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
}

impl Wordle {
    fn get_query_parts(&self, message_id: u64) -> Vec<Box<dyn ToSql>> {
        let mut ret: Vec<Box<dyn ToSql>> = Vec::new();

        ret.push(Box::new(message_id));
        ret.push(Box::new(self.number));
        ret.push(Box::new(self.score));
        ret.push(Box::new(self.hardmode));

        for element in self.board {
            ret.push(Box::new(element));
        }
        ret
    }
}
