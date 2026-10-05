use crate::structs::Message;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
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

/// Number of chunks an image hash is split into for finding near matches.
///
/// By the pigeonhole principle, two hashes that differ in fewer bits than
/// there are chunks must have at least one identical chunk, so looking up
/// hashes sharing any chunk finds every hash within that distance. Each
/// chunk is stored in its own indexed column, `h1` to `h5` of `image`.
pub const HASH_CHUNKS: usize = 5;

/// Splits a base64 encoded image hash into [`HASH_CHUNKS`] integers of
/// near equal size, each chunk being the big endian value of its bytes.
pub(crate) fn hash_chunks(hash: &str) -> Result<[i64; HASH_CHUNKS]> {
    let invalid = |why: String| Error::ToSqlConversionFailure(why.into());
    let bytes = BASE64
        .decode(hash)
        .map_err(|why| invalid(format!("image hash {hash:?} is not base64: {why}")))?;
    // chunks must fit in an i64 to be stored as an sqlite integer
    if bytes.len() < HASH_CHUNKS || bytes.len() > HASH_CHUNKS * 7 {
        return Err(invalid(format!(
            "image hash {hash:?} must decode to {HASH_CHUNKS} to {} bytes",
            HASH_CHUNKS * 7
        )));
    }

    let (size, remainder) = (bytes.len() / HASH_CHUNKS, bytes.len() % HASH_CHUNKS);
    let mut chunks = [0; HASH_CHUNKS];
    let mut start = 0;
    for (i, chunk) in chunks.iter_mut().enumerate() {
        // the first chunks take one extra byte each when the bytes don't divide evenly
        let end = start + size + usize::from(i < remainder);
        *chunk = bytes[start..end]
            .iter()
            .fold(0, |acc, byte| (acc << 8) | i64::from(*byte));
        start = end;
    }
    Ok(chunks)
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

    fn encode(bytes: &[u8]) -> String {
        BASE64.encode(bytes)
    }

    #[test]
    fn test_hash_chunks_split() {
        // a 256 bit hash splits into chunks of 7, 7, 6, 6, 6 bytes
        let bytes: Vec<u8> = (1..=32).collect();
        let chunks = hash_chunks(&encode(&bytes)).unwrap();
        assert_eq!(
            chunks,
            [
                0x01_02_03_04_05_06_07,
                0x08_09_0a_0b_0c_0d_0e,
                0x0f_10_11_12_13_14,
                0x15_16_17_18_19_1a,
                0x1b_1c_1d_1e_1f_20,
            ]
        );
    }

    #[test]
    fn test_hash_chunks_max_values_fit() {
        let chunks = hash_chunks(&encode(&[0xff; 35])).unwrap();
        assert!(chunks.iter().all(|chunk| *chunk == 0x00ff_ffff_ffff_ffff));
    }

    #[test]
    fn test_hash_chunks_rejects_invalid() {
        assert!(hash_chunks("not base64!").is_err());
        assert!(hash_chunks(&encode(&[1, 2, 3, 4])).is_err());
        assert!(hash_chunks(&encode(&[0; 36])).is_err());
        assert!(hash_chunks(&encode(&[0; 5])).is_ok());
    }

    #[test]
    fn test_hashes_within_distance_share_a_chunk() {
        // flipping fewer bits than there are chunks always leaves one untouched,
        // whichever bits are flipped
        let bytes: Vec<u8> = (100..132).collect();
        let original = hash_chunks(&encode(&bytes)).unwrap();
        for bits in [[0, 60, 120, 200], [7, 8, 9, 255], [55, 56, 111, 112]] {
            let mut flipped = bytes.clone();
            for bit in bits {
                flipped[bit / 8] ^= 1 << (bit % 8);
            }
            let chunks = hash_chunks(&encode(&flipped)).unwrap();
            assert!(
                original.iter().zip(chunks).any(|(a, b)| *a == b),
                "no shared chunk flipping bits {bits:?}"
            );
        }
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
