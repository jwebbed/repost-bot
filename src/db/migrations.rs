use super::queries;
use crate::errors::Result;
use log::{info, warn};
use rusqlite::Connection;

const MIGRATION_1: [&str; 6] = [
    // add server table
    "CREATE TABLE server ( 
        id INTEGER PRIMARY KEY, 
        name TEXT
    );",
    // add channel table
    "CREATE TABLE channel ( 
        id INTEGER PRIMARY KEY, 
        name TEXT,
        server INTEGER,
        FOREIGN KEY(server) REFERENCES server(id)
    );",
    // add message table
    "CREATE TABLE message (
        id INTEGER PRIMARY KEY,
        server INTEGER,
        channel INTEGER,
        created_at NUMERIC,
        FOREIGN KEY(server) REFERENCES server(id),
        FOREIGN KEY(channel) REFERENCES channel(id)
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
             FOREIGN KEY(link) REFERENCES link(id),
             FOREIGN KEY(message) REFERENCES message(id)
         );",
    // add link table index
    "CREATE UNIQUE INDEX idx_message_link ON message_link (link, message);",
];

fn migration_1(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_1 {
        conn.execute(migration, [])?;
    }
    queries::set_version(conn, 1)?;
    Ok(())
}

const MIGRATION_2: [&str; 1] = ["ALTER TABLE channel ADD visible BOOLEAN DEFAULT TRUE;"];

fn migration_2(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_2 {
        conn.execute(migration, [])?;
    }
    queries::set_version(conn, 2)?;
    Ok(())
}

const MIGRATION_3: [&str; 13] = [
    // add temp channel table
    "CREATE TABLE channel_temp ( 
        id INTEGER PRIMARY KEY, 
        name TEXT,
        visible BOOLEAN,
        server INTEGER,
        FOREIGN KEY(server) REFERENCES server(id) ON DELETE CASCADE
    );",
    // add message table
    "CREATE TABLE message_temp (
        id INTEGER PRIMARY KEY,
        server INTEGER,
        channel INTEGER,
        created_at NUMERIC,
        FOREIGN KEY(channel) REFERENCES channel_temp(id) ON DELETE CASCADE
    );",
    // create message_link table to connect links and the messages they're posted in
    "CREATE TABLE message_link_temp (
        id INTEGER PRIMARY KEY,
        link INTEGER NOT NULL,
        message INTEGER NOT NULL,
        FOREIGN KEY(link) REFERENCES link(id) ON DELETE CASCADE,
        FOREIGN KEY(message) REFERENCES message_temp(id) ON DELETE CASCADE
    );",
    // Insert old tables entries into new temp tables
    "INSERT INTO channel_temp (id, name, visible, server)
    SELECT id, name, visible, server FROM channel",
    "INSERT INTO message_temp (id, server, channel, created_at)
    SELECT id, server, channel, created_at FROM message",
    "INSERT INTO message_link_temp (id, link, message)
    SELECT id, link, message FROM message_link",
    // Drop old message_link and rename temp
    "DROP TABLE message_link",
    "ALTER TABLE message_link_temp RENAME TO message_link",
    // Drop old message and rename temp
    "DROP TABLE message",
    "ALTER TABLE message_temp RENAME TO message",
    // Drop old channel and rename temp
    "DROP TABLE channel",
    "ALTER TABLE channel_temp RENAME TO channel",
    // add link table index
    "CREATE UNIQUE INDEX idx_message_link ON message_link (link, message);",
];

// This migration essentially re-does the basic tables, adding a ON DELETE CASCADE
// to all of the foreign key relations so we don't have to do all this
// stuff we manually deleting in reverse order.
//
// Less obviously it adds a ON DELETE CASCADE to the link relation in message_link,
// which is odd as there is currently no place where we delete a link that has a
// message link at the moment. This is primary for if we decide to remove some
// links from the link table because we decided we want to filter them out,
// we don't have to manually also remove this.
fn migration_3(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_3 {
        conn.execute(migration, [])?;
    }
    queries::set_version(conn, 3)?;
    Ok(())
}

const MIGRATION_4: [&str; 6] = [
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
    "CREATE UNIQUE INDEX idx_wordle_user ON wordle (message, score, hardmode);",
    "ALTER TABLE message ADD author INTEGER DEFAULT NULL;",
    "ALTER TABLE message ADD parsed_repost BOOLEAN DEFAULT FALSE;",
    "ALTER TABLE message ADD parsed_wordle BOOLEAN DEFAULT FALSE;",
    // Want the default to be FALSE for all new entries, but all
    // existing at time of migration should be TRUE
    "UPDATE message SET parsed_repost = TRUE;",
];

fn migration_4(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_4 {
        conn.execute(migration, [])?;
    }
    queries::set_version(conn, 4)?;
    Ok(())
}

const MIGRATION_5: [&str; 5] = [
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
    "CREATE INDEX idx_user ON user (username, discriminator, bot);",
    "CREATE UNIQUE INDEX idx_nickname ON nickname (server, user, nickname);",
    "CREATE INDEX idx_msg ON message (server, channel, author);",
];

fn migration_5(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_5 {
        conn.execute(migration, [])?;
    }
    queries::set_version(conn, 5)?;
    Ok(())
}

const MIGRATION_6: [&str; 1] = ["ALTER TABLE message ADD deleted BOOLEAN DEFAULT FALSE;"];

fn migration_6(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_6 {
        conn.execute(migration, [])?;
    }
    queries::set_version(conn, 6)?;
    Ok(())
}

fn delete_old_links(conn: &Connection) -> Result<()> {
    conn.execute(
        "DELETE FROM link WHERE id IN (
            SELECT L.id FROM link as L 
            LEFT JOIN message_link as ML 
            ON L.id = ML.link 
            WHERE ML.id IS NULL 
        );",
        [],
    )?;
    Ok(())
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    // be sure to increment this everytime a new migration is added
    const FINAL_VER: u32 = 6;

    let ver = queries::get_version(conn)?;

    if ver == FINAL_VER {
        info!(
            "database version {} which matches final ver {}, no need to migrate",
            ver, FINAL_VER
        );
        return Ok(());
    }

    let tx = conn.transaction()?;
    if ver < 1 {
        migration_1(&tx)?;
    }
    if ver < 2 {
        migration_2(&tx)?;
    }
    if ver < 3 {
        migration_3(&tx)?;
    }
    if ver < 4 {
        migration_4(&tx)?;
    }
    if ver < 5 {
        migration_5(&tx)?;
    }
    if ver < 6 {
        migration_6(&tx)?;
    }

    // delete old links we don't need
    delete_old_links(&tx)?;

    tx.commit()?;

    warn!("migration successful");
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
    fn test_message_table() -> Result<()> {
        let table = get_table_info("message")?;

        assert_eq!(table.rows.len(), 8);
        table.assert_row("id", "INTEGER", 0, None, 1);
        table.assert_row("server", "INTEGER", 0, None, 0);
        table.assert_row("channel", "INTEGER", 0, None, 0);
        table.assert_row("created_at", "NUMERIC", 0, None, 0);
        table.assert_row("author", "INTEGER", 0, Some("NULL"), 0);
        table.assert_row("parsed_repost", "BOOLEAN", 0, Some("FALSE"), 0);
        table.assert_row("parsed_wordle", "BOOLEAN", 0, Some("FALSE"), 0);
        table.assert_row("deleted", "BOOLEAN", 0, Some("FALSE"), 0);
        Ok(())
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
}
