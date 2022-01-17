use crate::structs::Message;
use rusqlite::{Connection, Result};

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

pub fn get_message(conn: &Connection, msg_id: u64) -> Result<Message> {
    // let mut stmt = conn.prepare(
    const QUERY: &str =
        "SELECT id, server, channel, author, created_at, parsed_repost, parsed_wordle
        FROM message WHERE id=(?1)";
    conn.query_row(QUERY, [msg_id], |row| {
        Ok(Message {
            id: row.get(0)?,
            server: row.get(1)?,
            channel: row.get(2)?,
            author: row.get(3)?,
            created_at: row.get(4)?,
            parsed_repost: row.get(5)?,
            parsed_wordle: row.get(6)?,
        })
    })
}
