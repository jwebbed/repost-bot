mod migrations;
mod queries;
mod read_only_db;
pub mod structs;
#[cfg(test)]
mod tests;
mod writeable_db;

pub use read_only_db::ReadOnlyDb;
pub use writeable_db::WriteableDb;

use rusqlite::{Connection, OpenFlags, Result};

pub(crate) mod connections {
    use rusqlite::{Connection, Params, Result};

    pub trait GetConnectionImmutable {
        fn get_connection(&self) -> &Connection;

        #[inline]
        fn execute<P: Params>(&self, sql: &str, params: P) -> Result<()> {
            self.get_connection().prepare_cached(sql)?.execute(params)?;
            Ok(())
        }
    }

    pub trait GetConnectionMutable {
        fn get_mutable_connection(&mut self) -> &mut Connection;
    }
}

pub struct ReadOnlyConn {
    conn: Connection,
}

impl connections::GetConnectionImmutable for ReadOnlyConn {
    #[inline]
    fn get_connection(&self) -> &Connection {
        &self.conn
    }
}

impl ReadOnlyDb for ReadOnlyConn {}

pub struct WriteableConn {
    conn: Connection,
}

impl connections::GetConnectionImmutable for WriteableConn {
    #[inline]
    fn get_connection(&self) -> &Connection {
        &self.conn
    }
}

impl connections::GetConnectionMutable for WriteableConn {
    #[inline]
    fn get_mutable_connection(&mut self) -> &mut Connection {
        &mut self.conn
    }
}

impl ReadOnlyDb for WriteableConn {}

impl WriteableDb for WriteableConn {}

const DB_PATH: &str = "./repost.db3";
const IN_MEMORY_DB: bool = false;

fn open_database(read_only: bool) -> Result<Connection> {
    match (IN_MEMORY_DB, read_only) {
        (true, true) => Connection::open_in_memory_with_flags(OpenFlags::SQLITE_OPEN_READ_ONLY),
        (true, false) => Connection::open_in_memory(),
        (false, true) => Connection::open_with_flags(DB_PATH, OpenFlags::SQLITE_OPEN_READ_ONLY),
        (false, false) => Connection::open(DB_PATH),
    }
}

impl ReadOnlyConn {
    #[inline]
    fn new() -> Result<ReadOnlyConn> {
        Ok(ReadOnlyConn {
            conn: open_database(true)?,
        })
    }
}

impl WriteableConn {
    #[inline]
    fn new() -> Result<WriteableConn> {
        Ok(WriteableConn {
            conn: open_database(false)?,
        })
    }

    /// A fully migrated, private, in-memory database for tests
    #[cfg(test)]
    pub(crate) fn new_in_memory() -> Result<WriteableConn> {
        let mut conn = Connection::open_in_memory()?;
        migrations::migrate(&mut conn)?;
        Ok(WriteableConn { conn })
    }
}

#[inline]
pub fn get_read_only_db() -> Result<impl ReadOnlyDb> {
    ReadOnlyConn::new()
}

#[inline]
pub fn get_writeable_db() -> Result<impl WriteableDb> {
    WriteableConn::new()
}

#[inline]
pub fn migrate() -> Result<()> {
    migrations::migrate(&mut open_database(false)?)
}

#[inline]
pub fn writable_db_call<F, T>(f: F) -> Result<T>
where
    F: FnOnce(WriteableConn) -> Result<T>,
{
    f(WriteableConn::new()?)
}

#[inline]
pub fn read_only_db_call<F, T>(f: F) -> Result<T>
where
    F: FnOnce(ReadOnlyConn) -> Result<T>,
{
    f(ReadOnlyConn::new()?)
}
