use super::queries;
use crate::errors::Result;
use rusqlite::Connection;
//use std::collections::HashMap;

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
    const FINAL_VER: u32 = 3;

    let ver = queries::get_version(conn)?;

    if ver == FINAL_VER {
        println!(
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

    // delete old links we don't need
    delete_old_links(&tx)?;

    tx.commit()?;

    println!("migration successful");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use std::collections::HashMap;

    #[derive(Debug, Clone)]
    struct TableInfo {
        pub cid: usize,
        pub name: String,
        pub type_name: String,
        pub not_null: usize,
        pub default_value: Option<String>,
        pub pk: usize,
    }

    fn get_migrated_db() -> Result<Connection> {
        let mut conn = Connection::open_in_memory()?;
        migrate(&mut conn)?;
        Ok(conn)
    }

    fn get_table_info(table_name: &str) -> Result<HashMap<String, TableInfo>> {
        let conn = get_migrated_db()?;
        let mut stmt = conn.prepare("SELECT * FROM pragma_table_info(?1);")?;
        let rows = stmt.query_map(params![table_name], |row| {
            Ok(TableInfo {
                cid: row.get(0)?,
                name: row.get(1)?,
                type_name: row.get(2)?,
                not_null: row.get(3)?,
                default_value: row.get(4)?,
                pk: row.get(5)?,
            })
        })?;

        let mut m = HashMap::new();
        for row in rows {
            let info = row?;
            m.insert(info.name.clone(), info);
        }
        Ok(m)
    }

    #[test]
    fn test_channel_table() -> Result<()> {
        let table_info = get_table_info("channel")?;
        println!("{:#?}", table_info);

        assert!(table_info.contains_key("id"));
        assert!(table_info.contains_key("name"));
        assert!(table_info.contains_key("visible"));
        assert!(table_info.contains_key("server"));
        Ok(())
    }
    /*#[test]
    fn test_extract_link() {
        let links = get_links("test msg with link https://twitter.com/user/status/idnumber?s=20");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "https://twitter.com/user/status/idnumber?s=20");
    }*/
}
