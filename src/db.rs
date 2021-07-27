use crate::structs::Link;
use rusqlite::{params, Connection, Result};

pub fn get_db() -> Result<DB> {
    // set to true to test without migration issues
    const IN_MEMORY_DB: bool = false;
    Ok(DB {
        conn: if IN_MEMORY_DB {
            Connection::open_in_memory()?
        } else {
            let path = "./repost.db3";
            Connection::open(&path)?
        },
    })
}

#[derive(Debug)]
pub struct DB {
    pub conn: Connection,
}

impl DB {
    pub fn migrate(&self) -> Result<()> {
        // be sure to increment this everytime a new migration is added
        const FINAL_VER: u32 = 1;

        let ver = self.get_version()?;
        if ver == FINAL_VER {
            println!(
                "database version {} which matches final ver {}, no need to migrate",
                ver, FINAL_VER
            );
            return Ok(());
        }

        let mut migrations: Vec<&str> = Vec::new();
        if ver < 1 {
            // add server table
            migrations.push(
                "CREATE TABLE server ( 
                    id INTEGER PRIMARY KEY, 
                    name TEXT
                );",
            );
            // add channel table
            migrations.push(
                "CREATE TABLE channel ( 
                    id INTEGER PRIMARY KEY, 
                    name TEXT,
                    server INTEGER,
                    FOREIGN KEY(server) REFERENCES server(id)
                );",
            );

            // add message table
            migrations.push(
                "CREATE TABLE message (
                    id INTEGER PRIMARY KEY,
                    server INTEGER,
                    channel INTEGER,
                    FOREIGN KEY(server) REFERENCES server(id),
                    FOREIGN KEY(channel) REFERENCES channel(id)
                );",
            );

            // add link table
            migrations.push(
                "CREATE TABLE link (
                    id INTEGER PRIMARY KEY,
                    link TEXT NOT NULL,
                    server INTEGER,
                    channel INTEGER,
                    message INTEGER,
                    FOREIGN KEY(server) REFERENCES server(id),
                    FOREIGN KEY(channel) REFERENCES channel(id),
                    FOREIGN KEY(message) REFERENCES message(id)
                );",
            );
            // add link table index
            migrations.push("CREATE INDEX idx_link_link ON link (link);");
        }
        let final_migration = migrations.join("");
        println!("Running migrations: {}", final_migration);
        match self.conn.execute_batch(&final_migration) {
            Ok(_) => {
                println!("migration successful, setting final ver");
                self.set_version(FINAL_VER)?;
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    fn get_version(&self) -> Result<u32> {
        self.conn
            .query_row("SELECT user_version FROM pragma_user_version;", [], |row| {
                row.get(0)
            })
    }

    fn set_version(&self, version: u32) -> Result<()> {
        self.conn.pragma_update(None, "user_version", &version)
    }

    pub fn insert_link(&self, link: Link) -> Result<usize> {
        println!("Inserting the following link {:?}", link);

        self.conn.execute(
            "INSERT INTO link (link, server, channel, message) VALUES (?1, ?2, ?3, ?4)",
            params![link.link, link.server, link.channel, link.message],
        )
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
        let mut stmt = self.conn.prepare_cached(
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
        let mut stmt = self.conn.prepare_cached(
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
        let mut stmt = self.conn.prepare_cached(
            "SELECT L.id, L.link, L.server, L.channel, L.message, C.name, S.name
            FROM link AS L 
            JOIN channel AS C ON L.channel=C.id
            JOIN server AS S ON L.server=S.id
            WHERE L.link = (?1) AND L.server = (?2)",
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
}
