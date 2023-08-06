use crate::structs::Message;
use rusqlite::{Connection, OptionalExtension, Result};

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
pub fn get_message(conn: &Connection, msg_id: u64) -> Result<Option<Message>> {
    conn.query_row(
        "SELECT id, server, channel, author, created_at, 
        parsed_repost, deleted, checked_old, parsed_embed
        FROM message WHERE id=(?1)",
        [msg_id],
        |row| {
            Ok(Message::new(
                row.get(0)?, // id
                row.get(1)?, // server
                row.get(2)?, // channel
                row.get(3)?, // author
                row.get(4)?, // created_at
                row.get(5)?, // parsed_repost
                row.get(8)?, // parsed_embed
                row.get(6)?, // deleted
                row.get(7)?, // checked_old
            ))
        },
    )
    .optional()
}
