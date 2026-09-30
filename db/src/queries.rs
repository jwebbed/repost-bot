use crate::structs::Message;
use rusqlite::{Connection, Error, OptionalExtension, Result, Row};

/// Expands to the comma separated list of message columns, in the order
/// expected by [`message_from_row`], optionally prefixed with a table alias.
macro_rules! message_columns {
    () => {
        "id, server, channel, author, created_at, parsed_repost, parsed_embed, deleted, checked_old"
    };
    ($alias:literal) => {
        concat!(
            $alias,
            ".id, ",
            $alias,
            ".server, ",
            $alias,
            ".channel, ",
            $alias,
            ".author, ",
            $alias,
            ".created_at, ",
            $alias,
            ".parsed_repost, ",
            $alias,
            ".parsed_embed, ",
            $alias,
            ".deleted, ",
            $alias,
            ".checked_old"
        )
    };
}
pub(crate) use message_columns;

/// Number of columns produced by [`message_columns!`]
pub(crate) const MESSAGE_COLUMN_COUNT: usize = 9;

/// Builds a [`Message`] from a row selected with [`message_columns!`],
/// starting at column index `start`.
#[inline]
pub(crate) fn message_from_row(row: &Row<'_>, start: usize) -> Result<Message> {
    Ok(Message::new(
        row.get(start)?,
        row.get(start + 1)?,
        row.get(start + 2)?,
        row.get(start + 3)?,
        row.get(start + 4)?,
        row.get(start + 5)?,
        row.get(start + 6)?,
        row.get(start + 7)?,
        row.get(start + 8)?,
    ))
}

/// Byte offsets of the characters of a base64 image hash that are stored in
/// the `c1`..`c5` columns of the `image` table and used to find near matches.
///
/// These offsets are baked into existing rows, so they must never change
/// without a migration that recomputes the columns.
const HASH_SAMPLE_OFFSETS: [usize; 5] = [0, 2, 5, 9, 14];

/// Returns the single character samples of `hash` stored in `c1`..`c5`.
pub(crate) fn hash_samples(hash: &str) -> Result<[&str; 5]> {
    let min_len = HASH_SAMPLE_OFFSETS[HASH_SAMPLE_OFFSETS.len() - 1] + 1;
    if !hash.is_ascii() || hash.len() < min_len {
        return Err(Error::ToSqlConversionFailure(
            format!("image hash {hash:?} must be ascii and at least {min_len} characters").into(),
        ));
    }
    Ok(HASH_SAMPLE_OFFSETS.map(|i| &hash[i..=i]))
}

#[inline]
pub(crate) fn get_version(conn: &Connection) -> Result<u32> {
    conn.query_row("SELECT user_version FROM pragma_user_version;", [], |row| {
        row.get(0)
    })
}

#[inline]
pub(crate) fn set_version(conn: &Connection, version: u32) -> Result<()> {
    conn.pragma_update(None, "user_version", version)
}

#[inline]
pub(crate) fn get_message(conn: &Connection, msg_id: u64) -> Result<Option<Message>> {
    conn.prepare_cached(concat!(
        "SELECT ",
        message_columns!(),
        " FROM message WHERE id=(?1)"
    ))?
    .query_row([msg_id], |row| message_from_row(row, 0))
    .optional()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_samples_match_legacy_sampling() {
        // The original implementation sampled with chained `nth` calls,
        // ensure the offsets still line up with that behaviour
        let hash = "MuNy4INik8O0wRjlGjZNdmlPbA9kO9f50/hDek3aRcY=";
        let mut chars = hash.chars();
        let legacy = [
            chars.next().unwrap().to_string(),
            chars.nth(1).unwrap().to_string(),
            chars.nth(2).unwrap().to_string(),
            chars.nth(3).unwrap().to_string(),
            chars.nth(4).unwrap().to_string(),
        ];
        assert_eq!(
            hash_samples(hash).unwrap(),
            legacy.each_ref().map(String::as_str)
        );
    }

    #[test]
    fn test_hash_samples_rejects_short_hash() {
        assert!(hash_samples("abcdefghijklmn").is_err());
        assert!(hash_samples("abcdefghijklmno").is_ok());
    }

    #[test]
    fn test_hash_samples_rejects_non_ascii() {
        assert!(hash_samples("ééééééééééééééé").is_err());
    }

    #[test]
    fn test_message_columns_count() {
        assert_eq!(message_columns!().split(',').count(), MESSAGE_COLUMN_COUNT);
        assert_eq!(
            message_columns!("M").split(',').count(),
            MESSAGE_COLUMN_COUNT
        );
    }
}
