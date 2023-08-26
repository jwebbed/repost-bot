mod migrations;
mod queries;
mod read_only_db;
pub mod structs;
mod writeable_db;

pub use read_only_db::ReadOnlyDb;
pub use writeable_db::WriteableDb;

use rusqlite::{Connection, OpenFlags, Result};

pub(crate) mod connections {
    use rusqlite::{Connection, Params, Result};

    pub trait GetConnectionImmutable {
        fn get_connection(&self) -> &Connection;

        #[inline(always)]
        fn execute<P: Params>(&self, sql: &str, params: P) -> Result<()> {
            self.get_connection().execute(sql, params)?;
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

#[inline(always)]
fn open_database_ro() -> Result<Connection> {
    if IN_MEMORY_DB {
        Connection::open_in_memory_with_flags(OpenFlags::SQLITE_OPEN_READ_ONLY)
    } else {
        Connection::open_with_flags(DB_PATH, OpenFlags::SQLITE_OPEN_READ_ONLY)
    }
}

#[inline(always)]
fn open_database_rw() -> Result<Connection> {
    if IN_MEMORY_DB {
        Connection::open_in_memory()
    } else {
        Connection::open(DB_PATH)
    }
}

#[inline(always)]
fn open_database(read_only: bool) -> Result<Connection> {
    if read_only {
        open_database_ro()
    } else {
        open_database_rw()
    }
}

impl ReadOnlyConn {
    #[inline(always)]
    fn new() -> Result<ReadOnlyConn> {
        Ok(ReadOnlyConn {
            conn: open_database(true)?,
        })
    }
}

impl WriteableConn {
    #[inline(always)]
    fn new() -> Result<WriteableConn> {
        Ok(WriteableConn {
            conn: open_database(false)?,
        })
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
