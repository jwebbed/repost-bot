use crate::structs::Link;
use num::Integer;
use rusqlite::types::Value;
use rusqlite::{params, Connection, Result};
use std::rc::Rc;

#[inline(always)]
pub fn get_version(conn: &Connection) -> Result<u32> {
    conn.query_row("SELECT user_version FROM pragma_user_version;", [], |row| {
        row.get(0)
    })
}

#[inline(always)]
pub fn set_version(conn: &Connection, version: u32) -> Result<()> {
    conn.pragma_update(None, "user_version", &version)
}

#[inline(always)]
pub fn insert_query<I: Integer + rusqlite::ToSql>(
    conn: &Connection,
    field: &str,
    value: &str,
    link: I,
) -> Result<usize> {
    conn.execute(
        "INSERT INTO query (field, value, link) VALUES (?1, ?2, ?3)",
        params![field, value, link],
    )
}

#[inline(always)]
fn map_link_rows(row: &rusqlite::Row) -> Result<Link> {
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
}

#[inline(always)]
pub fn query_links_on_host_path(
    conn: &Connection,
    host: &str,
    path: &str,
    server: u64,
) -> Result<Vec<Link>> {
    let mut stmt = conn.prepare(
        "SELECT L.id, L.link, L.server, L.channel, L.message, C.name, S.name
        FROM link as L
        JOIN channel AS C ON L.channel=C.id
        JOIN server AS S ON L.server=S.id
        WHERE L.host=(?1) AND L.path=(?2) AND L.server=(?3);",
    )?;

    let mut links = Vec::new();
    for row in stmt.query_map(params![host, path, server], map_link_rows)? {
        links.push(row?)
    }

    Ok(links)
}

#[inline(always)]
pub fn query_links_on_id_vector(conn: &Connection, ids: &Vec<i64>) -> Result<Vec<Link>> {
    let mut stmt = conn.prepare(
        "SELECT L.id, L.link, L.server, L.channel, L.message, C.name, S.name
        FROM link as L
        JOIN channel AS C ON L.channel=C.id
        JOIN server AS S ON L.server=S.id
        WHERE L.id IN rarray(?1);",
    )?;

    let mut links = Vec::new();
    for row in stmt.query_map(
        params![Rc::new(
            ids.iter().copied().map(Value::from).collect::<Vec<Value>>()
        )],
        map_link_rows,
    )? {
        links.push(row?)
    }

    Ok(links)
}
