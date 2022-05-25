use super::queries;
use log::{info, trace};

use rusqlite::{Connection, Result};

macro_rules! migration {
    ( $n:literal, $( $x:literal ),* ) => {
        paste::item! {
            fn [< migration_$n >] (conn: &Connection) -> Result<()> {
                trace!("running migration {}", $n);

                $(
                    conn.execute($x, [])?;
                )*
                queries::set_version(conn, $n)?;
                trace!("finished migration {}", $n);
                Ok(())
            }
        }
    };
}

migration![
    7,
    // add server table
    "CREATE TABLE server (
        id INTEGER PRIMARY KEY,
        name TEXT
    );",
    // add temp channel table
    "CREATE TABLE channel ( 
        id INTEGER PRIMARY KEY, 
        name TEXT,
        visible BOOLEAN,
        server INTEGER,
        FOREIGN KEY(server) REFERENCES server(id) ON DELETE CASCADE
    );",
    "CREATE TABLE message (
        id INTEGER PRIMARY KEY,
        server INTEGER,
        channel INTEGER,
        created_at NUMERIC,
        author INTEGER DEFAULT NULL,
        parsed_repost NUMERIC DEFAULT NULL,
        parsed_wordle NUMERIC DEFAULT NULL,
        deleted NUMERIC DEFAULT NULL,
        checked_old NUMERIC DEFAULT NULL,
        FOREIGN KEY(channel) REFERENCES channel(id) ON DELETE CASCADE
    );",
    // add link table
    "CREATE TABLE link (
        id INTEGER PRIMARY KEY,
        link TEXT NOT NULL UNIQUE
    );",
    // create message_link table to connect links and the messages they're posted in
    "CREATE TABLE message_link (
        id INTEGER PRIMARY KEY,
        link INTEGER NOT NULL,
        message INTEGER NOT NULL,
        FOREIGN KEY(link) REFERENCES link(id) ON DELETE CASCADE,
        FOREIGN KEY(message) REFERENCES message(id) ON DELETE CASCADE
    );",
    "CREATE TABLE user ( 
        id INTEGER PRIMARY KEY, 
        username TEXT NOT NULL,
        bot BOOL NOT NULL,
        discriminator INTEGER NOT NULL
    );",
    "CREATE TABLE nickname ( 
        nickname TEXT NOT NULL,
        user INTEGER NOT NULL,
        server INTEGER NOT NULL,
        PRIMARY KEY (user, nickname, server),
        FOREIGN KEY(server) REFERENCES server(id) ON DELETE CASCADE,
        FOREIGN KEY(user) REFERENCES user(id) ON DELETE CASCADE
    );",
    "CREATE TABLE wordle ( 
        message INTEGER PRIMARY KEY,
        number INTEGER NOT NULL,
        score INTEGER NOT NULL,
        hardmode BOOLEAN NOT NULL,

        board_r1c1 INTEGER NOT NULL,
        board_r1c2 INTEGER NOT NULL,
        board_r1c3 INTEGER NOT NULL,
        board_r1c4 INTEGER NOT NULL,
        board_r1c5 INTEGER NOT NULL,

        board_r2c1 INTEGER NOT NULL,
        board_r2c2 INTEGER NOT NULL,
        board_r2c3 INTEGER NOT NULL,
        board_r2c4 INTEGER NOT NULL,
        board_r2c5 INTEGER NOT NULL,

        board_r3c1 INTEGER NOT NULL,
        board_r3c2 INTEGER NOT NULL,
        board_r3c3 INTEGER NOT NULL,
        board_r3c4 INTEGER NOT NULL,
        board_r3c5 INTEGER NOT NULL,

        board_r4c1 INTEGER NOT NULL,
        board_r4c2 INTEGER NOT NULL,
        board_r4c3 INTEGER NOT NULL,
        board_r4c4 INTEGER NOT NULL,
        board_r4c5 INTEGER NOT NULL,

        board_r5c1 INTEGER NOT NULL,
        board_r5c2 INTEGER NOT NULL,
        board_r5c3 INTEGER NOT NULL,
        board_r5c4 INTEGER NOT NULL,
        board_r5c5 INTEGER NOT NULL,

        board_r6c1 INTEGER NOT NULL,
        board_r6c2 INTEGER NOT NULL,
        board_r6c3 INTEGER NOT NULL,
        board_r6c4 INTEGER NOT NULL,
        board_r6c5 INTEGER NOT NULL,

        FOREIGN KEY(message) REFERENCES message(id) ON DELETE CASCADE
    );",
    "CREATE INDEX idx_msg ON message (server, channel, author);",
    "CREATE INDEX idx_user ON user (username, discriminator, bot);",
    "CREATE UNIQUE INDEX idx_nickname ON nickname (server, user, nickname);",
    "CREATE UNIQUE INDEX idx_wordle_user ON wordle (message, score, hardmode);"
];

migration![
    8,
    "ALTER TABLE message ADD COLUMN parsed_embed NUMERIC DEFAULT NULL;",
    "CREATE TABLE image (
        id INTEGER PRIMARY KEY,
        url TEXT UNIQUE,
        c1 TEXT NOT NULL,
        c2 TEXT NOT NULL,
        c3 TEXT NOT NULL,
        c4 TEXT NOT NULL,
        c5 TEXT NOT NULL,
        hash TEXT NOT NULL
    );",
    "CREATE TABLE message_image (
        id INTEGER PRIMARY KEY,
        image INTEGER NOT NULL,
        message INTEGER NOT NULL,
        FOREIGN KEY(image) REFERENCES image(id) ON DELETE CASCADE,
        FOREIGN KEY(message) REFERENCES message(id) ON DELETE CASCADE
    );",
    "CREATE INDEX idx_image ON image (c1, c2, c3, c4, c5, hash);"
];

migration![
    9,
    "CREATE TABLE reply (
        id INTEGER PRIMARY KEY,
        channel INTEGER NOT NULL,
        replied_to INTEGER NOT NULL,
        FOREIGN KEY(channel) REFERENCES channel(id) ON DELETE CASCADE,
        FOREIGN KEY(replied_to) REFERENCES message(id) ON DELETE CASCADE
    );",
    "CREATE INDEX idx_reply ON reply (replied_to);"
];

fn delete_old_links(conn: &Connection) -> Result<()> {
    trace!("starting delete old links");
    conn.execute(
        "DELETE FROM link WHERE id IN (
            SELECT L.id FROM link as L 
            LEFT JOIN message_link as ML 
            ON L.id = ML.link 
            WHERE ML.id IS NULL 
        );",
        [],
    )?;
    // todo: make this info and include the number of links deleted
    // leave at trace as not super useful without number
    trace!("finished delete old links");
    Ok(())
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    const MIN_VER: u32 = 7;
    // be sure to increment this everytime a new migration is added
    const FINAL_VER: u32 = 9;

    let ver = queries::get_version(conn)?;
    info!("database version is currently: {ver} with target ver {FINAL_VER}");
    if ver == FINAL_VER {
        return Ok(());
    }
    if ver > 0 && ver < MIN_VER {
        panic!("database current version {ver}, cannot migrate a database < {MIN_VER} or not 0");
    }
    trace!("disabling foreign keys pre-migration");
    conn.pragma_update(None, "foreign_keys", "OFF")?;

    let tx = conn.transaction()?;

    trace!("starting migration transaction");

    if ver < 7 {
        migration_7(&tx)?;
    }

    if ver < 8 {
        migration_8(&tx)?;
    }

    if ver < 9 {
        migration_9(&tx)?;
    }
    // delete old links we don't need
    delete_old_links(&tx)?;

    trace!("commiting migration transaction");
    tx.commit()?;
    trace!("successfully commited migration transaction");

    conn.pragma_update(None, "foreign_keys", "ON")?;
    trace!("enabling foreign keys post-migration");
    info!("migration successful");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::cmp::{Eq, PartialEq};
    use std::collections::HashMap;

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ColumnInfo {
        pub name: String,
        pub type_name: String,
        pub notnull: usize,
        pub default: Option<String>,
        pub pk: usize,
    }

    struct Table {
        pub rows: HashMap<String, ColumnInfo>,
    }

    impl Table {
        fn assert_row(
            &self,
            name: &str,
            type_name: &str,
            notnull: usize,
            default: Option<&str>,
            pk: usize,
        ) {
            assert_eq!(
                &ColumnInfo {
                    name: String::from(name),
                    type_name: String::from(type_name),
                    notnull,
                    default: default.map(String::from),
                    pk,
                },
                self.rows.get(name).unwrap()
            );
        }
    }

    fn get_migrated_db() -> Result<Connection> {
        let mut conn = Connection::open_in_memory()?;
        migrate(&mut conn)?;
        Ok(conn)
    }

    fn get_table_info(table_name: &str) -> Result<Table> {
        let conn = get_migrated_db()?;
        let mut stmt = conn.prepare("SELECT * FROM pragma_table_info(?1);")?;
        let rows = stmt.query_map(params![table_name], |row| {
            Ok(ColumnInfo {
                name: row.get(1)?,
                type_name: row.get(2)?,
                notnull: row.get(3)?,
                default: row.get(4)?,
                pk: row.get(5)?,
            })
        })?;

        let mut m = HashMap::new();
        for row in rows {
            let info = row?;
            m.insert(info.name.clone(), info);
        }
        Ok(Table { rows: m })
    }

    #[test]
    fn test_channel_table() -> Result<()> {
        let ti = get_table_info("channel")?.rows;

        // Expect only 4 columns in channel table
        assert_eq!(ti.len(), 4);

        assert!(ti.contains_key("id"));
        assert!(ti.contains_key("name"));
        assert!(ti.contains_key("visible"));
        assert!(ti.contains_key("server"));
        Ok(())
    }

    #[test]
    fn test_link_table() -> Result<()> {
        let table = get_table_info("link")?;

        assert_eq!(table.rows.len(), 2);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("link", "TEXT", 1, None, 0);

        Ok(())
    }
    #[test]
    fn test_message_table() -> Result<()> {
        let table = get_table_info("message")?;

        assert_eq!(table.rows.len(), 10);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("server", "INTEGER", 0, None, 0);
        table.assert_row("channel", "INTEGER", 0, None, 0);
        table.assert_row("created_at", "NUMERIC", 0, None, 0);
        table.assert_row("author", "INTEGER", 0, Some("NULL"), 0);
        table.assert_row("parsed_repost", "NUMERIC", 0, Some("NULL"), 0);
        table.assert_row("parsed_wordle", "NUMERIC", 0, Some("NULL"), 0);
        table.assert_row("deleted", "NUMERIC", 0, Some("NULL"), 0);
        table.assert_row("checked_old", "NUMERIC", 0, Some("NULL"), 0);
        table.assert_row("parsed_embed", "NUMERIC", 0, Some("NULL"), 0);
        Ok(())
    }

    #[test]
    fn test_message_link_table() -> Result<()> {
        let table = get_table_info("message_link")?;

        assert_eq!(table.rows.len(), 3);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("link", "INTEGER", 1, None, 0);
        table.assert_row("message", "INTEGER", 1, None, 0);

        Ok(())
    }

    #[test]
    fn test_nickname_table() -> Result<()> {
        let table = get_table_info("nickname")?;

        assert_eq!(table.rows.len(), 3);
        // should change pk to be a bool as we really don't care the pk number
        // and only that it is actually a pk
        table.assert_row("nickname", "TEXT", 1, None, 2);
        table.assert_row("user", "INTEGER", 1, None, 1);
        table.assert_row("server", "INTEGER", 1, None, 3);

        Ok(())
    }

    #[test]
    fn test_server_table() -> Result<()> {
        let table = get_table_info("server")?;

        assert_eq!(table.rows.len(), 2);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("name", "TEXT", 0, None, 0);

        Ok(())
    }

    #[test]
    fn test_user_table() -> Result<()> {
        let table = get_table_info("user")?;

        assert_eq!(table.rows.len(), 4);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("username", "TEXT", 1, None, 0);
        table.assert_row("bot", "BOOL", 1, None, 0);
        table.assert_row("discriminator", "INTEGER", 1, None, 0);

        Ok(())
    }

    #[test]
    fn test_wordle_table() -> Result<()> {
        let table = get_table_info("wordle")?;

        assert_eq!(table.rows.len(), 4 + 5 * 6);
        table.assert_row("message", "INTEGER", 0, None, 1);
        table.assert_row("number", "INTEGER", 1, None, 0);
        table.assert_row("score", "INTEGER", 1, None, 0);
        table.assert_row("hardmode", "BOOLEAN", 1, None, 0);
        for row in 1..=6 {
            for col in 1..=5 {
                table.assert_row(&format!("board_r{row}c{col}"), "INTEGER", 1, None, 0);
            }
        }

        Ok(())
    }

    #[test]
    fn test_reply_table() -> Result<()> {
        let table = get_table_info("reply")?;

        assert_eq!(table.rows.len(), 3);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("channel", "INTEGER", 1, None, 0);
        table.assert_row("replied_to", "INTEGER", 1, None, 0);
        Ok(())
    }
}
