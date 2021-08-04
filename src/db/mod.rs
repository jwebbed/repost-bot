mod migrations;
mod queries;

use crate::structs::Link;
use rusqlite::types::Value;
use rusqlite::{params, Connection, Result};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use url::Url;

pub fn result_convert<T, E>(res: Result<T, E>) -> Result<T> {
    match res {
        Ok(x) => Ok(x),
        Err(_) => Err(rusqlite::Error::InvalidQuery),
    }
}

#[derive(Debug, Default)]
struct QueryParam {
    pub link_id: i64,
    pub field: String,
}

#[derive(Debug)]
pub struct DB {
    pub conn: Connection,
}

#[allow(dead_code)]
impl DB {
    fn get_connection() -> Result<Connection> {
        const IN_MEMORY_DB: bool = false;
        let db = if IN_MEMORY_DB {
            Connection::open_in_memory()?
        } else {
            let path = "./repost.db3";
            Connection::open(&path)?
        };

        rusqlite::vtab::array::load_module(&db)?;
        Ok(db)
    }
    pub fn get_db() -> Result<DB> {
        // set to true to test without migration issues
        Ok(DB {
            conn: DB::get_connection()?,
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

    fn get_version(&self) -> Result<u32> {
        queries::get_version(&self.conn)
    }

    fn set_version(&self, version: u32) -> Result<()> {
        queries::set_version(&self.conn, version)
    }

    pub fn insert_link(&self, link: Link) -> Result<()> {
        println!("Inserting the following link {:?}", link);
        let parsed = result_convert(Url::parse(&link.link))?;
        //last_insert_rowid
        self.conn.execute(
            "INSERT INTO link (link, server, channel, message, host, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![link.link, link.server, link.channel, link.message, parsed.host_str().ok_or(rusqlite::Error::InvalidQuery)?, parsed.path()]
        )?;

        let link_id = self.conn.last_insert_rowid();
        for query in parsed.query_pairs() {
            queries::insert_query(&self.conn, &query.0, &query.1, link_id)?;
        }

        Ok(())
    }

    pub fn update_server(&self, server_id: u64, name: &Option<String>) -> Result<()> {
        let mut stmt = self.conn.prepare_cached(
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
            Err(why) => Err(why),
        }
    }

    pub fn update_channel(&self, channel_id: u64, server_id: u64, name: String) -> Result<()> {
        let mut stmt = self.conn.prepare(
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
            Err(why) => Err(why),
        }
    }

    pub fn add_message(&self, message_id: u64, channel_id: u64, server_id: u64) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO message (id, server, channel) VALUES ( ?1, ?2, ?3 )
            ON CONFLICT(id) DO NOTHING",
        )?;

        match stmt.execute(params![message_id, server_id, channel_id]) {
            Ok(cnt) => {
                if cnt > 0 {
                    println!("Added message_id {} db", message_id);
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Err(why) => Err(why),
        }
    }

    pub fn query_links(&self, link: &str, server: u64) -> Result<Vec<Link>> {
        let mut stmt = self.conn.prepare(
            "SELECT L.id, L.link, L.server, L.channel, L.message, C.name, S.name
            FROM link AS L 
            JOIN channel AS C ON L.channel=C.id
            JOIN server AS S ON L.server=S.id
            WHERE L.link = (?1) AND L.server = (?2);",
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

    pub fn query_links_host_path_fields(
        &self,
        host: &str,
        path: &str,
        server: u64,
        fields: &Vec<String>,
    ) -> Result<Vec<Link>> {
        // If there are no fields, no point in spending time on the logic
        if fields.len() == 0 {
            return queries::query_links_on_host_path(&self.conn, host, path, server);
        }

        let mut stmt = self.conn.prepare(
            "SELECT L.id, Q.field
            FROM
            ( SELECT id, link
               FROM link 
               WHERE host=(?1) AND path=(?2) AND server=(?3) 
            ) as L
            JOIN query AS Q on Q.link = L.id
            WHERE Q.field IN rarray(?4);",
        )?;

        let arr = Rc::new(
            fields
                .iter()
                .cloned()
                .map(Value::from)
                .collect::<Vec<Value>>(),
        );
        let rows = stmt.query_map(params![host, path, server, arr], |row| {
            Ok(QueryParam {
                link_id: row.get(0)?,
                field: row.get(1)?,
                ..Default::default()
            })
        })?;

        let mut field_map: HashMap<i64, HashSet<String>> = HashMap::new();
        for row in rows {
            let r = row?;
            if !field_map.contains_key(&r.link_id) {
                field_map.insert(r.link_id, HashSet::new());
            }

            field_map.get_mut(&r.link_id).unwrap().insert(r.field);
        }
        let mut ids = Vec::new();
        for (id, set) in field_map {
            if set.len() == fields.len() {
                ids.push(id);
            }
        }
        queries::query_links_on_id_vector(&self.conn, &ids)
    }
}
