mod migrations;
mod queries;

use crate::errors::{Error, Result};
use crate::structs::{Link, RepostCount};
use rusqlite::{params, Connection};
use serenity::model::id::MessageId;
use std::cell::RefCell;

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

    pub fn update_channel(&self, channel_id: u64, server_id: u64, name: String) -> Result<()> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "INSERT INTO channel (id, name, server) VALUES ( ?1, ?2, ?3 )
            ON CONFLICT(id) DO NOTHING",
        )?;

        match stmt.execute(params![channel_id, name, server_id]) {
            Ok(cnt) => {
                if cnt > 0 {
                    println!("Added channel_id {} with name {} to db", channel_id, name);
                };

                Ok(())
            }
            Err(why) => Err(Error::from(why)),
        }
    }

    pub fn get_oldest_message(&self, channel_id: u64) -> Result<Option<u64>> {
        let conn = self.conn.borrow();
        let ret = conn.query_row(
            "SELECT id FROM message WHERE channel=(?1) ORDER BY created_at asc LIMIT 1",
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
    ) -> Result<bool> {
        let conn = self.conn.borrow();
        let mut stmt = conn.prepare(
            "INSERT INTO message (id, server, channel, created_at) VALUES ( ?1, ?2, ?3, ?4 )
            ON CONFLICT(id) DO NOTHING",
        )?;

        match stmt.execute(params![
            *message_id.as_u64(),
            server_id,
            channel_id,
            message_id.created_at()
        ]) {
            Ok(cnt) => {
                if cnt > 0 {
                    println!("Added message_id {} db", message_id);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(why) => Err(Error::from(why)),
        }
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
            "SELECT L.id, L.link, S.id, C.id, M.id, C.name, S.name
            FROM link AS L 
            JOIN message_link as ML on ML.link=L.id
            JOIN message as M on ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            WHERE L.link = (?1) AND S.id = (?2);",
        )?;
        let rows = stmt.query_map(params![link, server], |row| {
            Ok(Link {
                id: row.get(0)?,
                link: row.get(1)?,
                server: row.get(2)?,
                channel: row.get(3)?,
                message: row.get(4)?,
                channel_name: row.get(5)?,
                server_name: row.get(6)?,
                ..Default::default()
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
                WHERE M.server=(?1)
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
}
