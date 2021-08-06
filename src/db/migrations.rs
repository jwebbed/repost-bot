use super::queries;
use crate::errors::Result;
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
        oldest_message INTEGER,
        FOREIGN KEY(server) REFERENCES server(id)
    );",
    // add message table
    "CREATE TABLE message (
        id INTEGER PRIMARY KEY,
        server INTEGER,
        channel INTEGER,
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
    queries::set_version(&conn, 1)?;
    Ok(())
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    // be sure to increment this everytime a new migration is added
    const FINAL_VER: u32 = 1;

    let ver = queries::get_version(&conn)?;

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

    tx.commit()?;

    println!("migration successful");
    Ok(())
}
