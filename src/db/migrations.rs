use super::{queries, result_convert};
use crate::structs::Link;
use rusqlite::{params, Connection, Result};
use url::Url;

const MIGRATION_1: [&str; 5] = [
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
        FOREIGN KEY(server) REFERENCES server(id),
        FOREIGN KEY(channel) REFERENCES channel(id)
    );",
    // add link table
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
    // add link table index
    "CREATE INDEX idx_link_link ON link (link);",
];

fn migration_1(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_1 {
        conn.execute(migration, [])?;
    }
    queries::set_version(&conn, 1)?;
    Ok(())
}

const MIGRATION_2: [&str; 3] = [
    // add query table
    "CREATE TABLE query (
        id INTEGER PRIMARY KEY,
        field TEXT NOT NULL,
        value TEXT NOT NULL,
        link INTEGER,
        FOREIGN KEY(link) REFERENCES link(id)
    );",
    "ALTER TABLE link ADD host TEXT DEFAULT '';",
    "ALTER TABLE link ADD path TEXT DEFAULT '';",
];
fn get_all_links(conn: &Connection) -> Result<Vec<Link>> {
    let mut stmt = conn.prepare("SELECT id, link, server, channel, message FROM link")?;
    let rows = stmt.query_map([], |row| {
        Ok(Link {
            id: row.get(0)?,
            link: row.get(1)?,
            server: row.get(2)?,
            channel: row.get(3)?,
            message: row.get(4)?,
            ..Default::default()
        })
    })?;

    let mut links = Vec::new();
    for row in rows {
        links.push(row?)
    }

    Ok(links)
}

fn migration_2(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_2 {
        println!("executing {}", migration);
        conn.execute(migration, [])?;
    }

    let mut link_stmt = conn.prepare("UPDATE link SET host=(?2), path=(?3) WHERE id=(?1);")?;
    let links = get_all_links(conn)?;

    for link in links {
        let parsed = result_convert(Url::parse(&link.link))?;
        let link_id = link.id.ok_or(rusqlite::Error::InvalidQuery)?;
        for query in parsed.query_pairs() {
            queries::insert_query(conn, &query.0, &query.1, link_id)?;
        }
        link_stmt.execute(params![
            link_id,
            parsed.host_str().ok_or(rusqlite::Error::InvalidQuery)?,
            parsed.path()
        ])?;
    }

    queries::set_version(&conn, 2)?;
    Ok(())
}

const MIGRATION_3: [&str; 1] = [
    // add query table
    "CREATE INDEX idx_host_path ON link (server,host,path);",
];

fn migration_3(conn: &Connection) -> Result<()> {
    for migration in MIGRATION_3 {
        conn.execute(migration, [])?;
    }
    queries::set_version(&conn, 3)?;
    Ok(())
}

pub fn migrate(conn: &mut Connection) -> Result<()> {
    // be sure to increment this everytime a new migration is added
    const FINAL_VER: u32 = 3;

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
    if ver < 2 {
        migration_2(&tx)?;
    }
    if ver < 3 {
        migration_3(&tx)?;
    }

    tx.commit()?;

    println!("migration successful");
    Ok(())
}
