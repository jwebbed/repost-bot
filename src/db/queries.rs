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
