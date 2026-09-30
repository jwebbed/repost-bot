//! Behavioural tests for the queries in [`ReadOnlyDb`] and [`WriteableDb`]
//! run against a migrated in-memory database.

use crate::connections::GetConnectionImmutable;
use crate::structs::Message;
use crate::writeable_db::get_snowflake_creation_time;
use crate::{ReadOnlyDb, WriteableConn, WriteableDb};

use chrono::{TimeZone, Utc};
use rusqlite::Result;

const SERVER: u64 = 1;
const OTHER_SERVER: u64 = 2;
const CHANNEL: u64 = 10;
const HIDDEN_CHANNEL: u64 = 11;
const OTHER_SERVER_CHANNEL: u64 = 20;
const AUTHOR: u64 = 100;
const OTHER_AUTHOR: u64 = 101;

const HASH: &str = "MuNy4INik8O0wRjlGjZNdmlPbA9kO9f50/hDek3aRcY=";
const OTHER_HASH: &str = "2YXmlWYDvQiN0M7Gfw7ZPNi0mB2QKbF7MLYn5QEvAXM=";

/// Builds a message snowflake created `ms` milliseconds after the discord epoch
const fn snowflake(ms: u64) -> u64 {
    ms << 22
}

fn setup() -> Result<WriteableConn> {
    let db = WriteableConn::new_in_memory()?;
    db.update_server(SERVER, Some("server"))?;
    db.update_server(OTHER_SERVER, Some("other server"))?;
    db.update_channel(CHANNEL, SERVER, "general", true)?;
    db.update_channel(HIDDEN_CHANNEL, SERVER, "secret", false)?;
    db.update_channel(OTHER_SERVER_CHANNEL, OTHER_SERVER, "general", true)?;
    db.add_user(AUTHOR, "author", false)?;
    db.add_user(OTHER_AUTHOR, "other_author", false)?;
    Ok(db)
}

fn add_msg(db: &WriteableConn, ms: u64, channel: u64, server: u64, author: u64) -> Message {
    db.add_message(snowflake(ms), channel, server, author)
        .unwrap()
}

fn ids(messages: &[Message]) -> Vec<u64> {
    let mut ids: Vec<u64> = messages.iter().map(|m| m.id).collect();
    ids.sort_unstable();
    ids
}

#[test]
fn test_snowflake_creation_time() {
    // example snowflake from the discord developer documentation
    assert_eq!(
        get_snowflake_creation_time(175_928_847_299_117_063),
        Utc.with_ymd_and_hms(2016, 4, 30, 11, 18, 25).unwrap()
            + chrono::Duration::milliseconds(796)
    );
    assert_eq!(
        get_snowflake_creation_time(0),
        Utc.with_ymd_and_hms(2015, 1, 1, 0, 0, 0).unwrap()
    );
}

#[test]
fn test_add_and_get_message() -> Result<()> {
    let db = setup()?;
    let msg = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);

    assert_eq!(msg.id, snowflake(1_000));
    assert_eq!(msg.server, SERVER);
    assert_eq!(msg.channel, CHANNEL);
    assert_eq!(msg.author, Some(AUTHOR));
    assert_eq!(msg.created_at, get_snowflake_creation_time(msg.id));
    assert!(!msg.is_repost_parsed());
    assert!(!msg.is_embed_parsed());
    assert!(!msg.is_deleted());
    assert!(!msg.is_checked_old());

    let fetched = db.get_message(msg.id)?.expect("message should exist");
    assert_eq!(fetched.created_at, msg.created_at);
    assert_eq!(db.get_message(snowflake(2_000))?, None);
    Ok(())
}

#[test]
fn test_add_message_twice_is_upsert() -> Result<()> {
    let db = setup()?;
    let first = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let second = add_msg(&db, 1_000, CHANNEL, SERVER, OTHER_AUTHOR);
    // author is only filled in when previously missing
    assert_eq!(first.author, second.author);
    Ok(())
}

#[test]
fn test_message_flags() -> Result<()> {
    let db = setup()?;
    let id = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR).id;

    db.mark_message_all_checked(id)?;
    let msg = db.get_message(id)?.unwrap();
    assert!(msg.is_repost_parsed());
    assert!(msg.is_embed_parsed());
    assert!(!msg.is_checked_old());

    db.mark_message_checked_old(id)?;
    assert!(db.get_message(id)?.unwrap().is_checked_old());

    db.soft_delete_message(id)?;
    assert!(db.get_message(id)?.unwrap().is_deleted());

    db.delete_message(id)?;
    assert_eq!(db.get_message(id)?, None);
    Ok(())
}

#[test]
fn test_update_server_only_fills_missing_name() -> Result<()> {
    let db = WriteableConn::new_in_memory()?;
    let name = |db: &WriteableConn| -> Result<Option<String>> {
        db.get_connection()
            .query_row("SELECT name FROM server WHERE id = ?1", [SERVER], |r| {
                r.get(0)
            })
    };

    db.update_server(SERVER, None)?;
    assert_eq!(name(&db)?, None);
    db.update_server(SERVER, Some("first"))?;
    assert_eq!(name(&db)?.as_deref(), Some("first"));
    db.update_server(SERVER, Some("second"))?;
    assert_eq!(name(&db)?.as_deref(), Some("first"));
    Ok(())
}

#[test]
fn test_channels() -> Result<()> {
    let db = setup()?;

    let mut list = db.get_channel_list(SERVER)?;
    list.sort_unstable();
    assert_eq!(
        list,
        vec![
            (CHANNEL, "general".to_string()),
            (HIDDEN_CHANNEL, "secret".to_string())
        ]
    );

    let known = db.get_known_channels(SERVER)?;
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].id, CHANNEL);
    assert_eq!(known[0].name.as_deref(), Some("general"));
    assert!(known[0].visible);
    assert_eq!(known[0].server, SERVER);

    db.update_channel_visibility(HIDDEN_CHANNEL, true)?;
    assert_eq!(db.get_known_channels(SERVER)?.len(), 2);

    db.update_channel(CHANNEL, SERVER, "renamed", true)?;
    assert!(
        db.get_channel_list(SERVER)?
            .contains(&(CHANNEL, "renamed".to_string()))
    );

    db.delete_channel(HIDDEN_CHANNEL)?;
    assert_eq!(db.get_channel_list(SERVER)?.len(), 1);
    Ok(())
}

#[test]
fn test_query_links() -> Result<()> {
    let mut db = setup()?;
    let link = "https://example.com/a";
    let original = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let hidden = add_msg(&db, 2_000, HIDDEN_CHANNEL, SERVER, AUTHOR);
    let other_server = add_msg(&db, 3_000, OTHER_SERVER_CHANNEL, OTHER_SERVER, AUTHOR);
    let deleted = add_msg(&db, 4_000, CHANNEL, SERVER, AUTHOR);
    for msg in [original, hidden, other_server, deleted] {
        db.insert_links([link], msg.id)?;
    }
    db.soft_delete_message(deleted.id)?;

    let links = db.query_links(link, SERVER)?;
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].link, link);
    assert_eq!(links[0].message, original);
    assert_eq!(links[0].message.author, Some(AUTHOR));
    assert_eq!(links[0].channel_name.as_deref(), Some("general"));
    assert_eq!(links[0].server_name.as_deref(), Some("server"));

    assert!(
        db.query_links("https://example.com/other", SERVER)?
            .is_empty()
    );
    assert_eq!(db.query_links(link, OTHER_SERVER)?.len(), 1);
    Ok(())
}

#[test]
fn test_insert_links_dedupes_per_message() -> Result<()> {
    let mut db = setup()?;
    let msg = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let link = "https://example.com/a";
    db.insert_links([link, link], msg.id)?;
    db.insert_links([link], msg.id)?;

    let count: u64 =
        db.get_connection()
            .query_row("SELECT COUNT(*) FROM message_link", [], |r| r.get(0))?;
    assert_eq!(count, 1);
    Ok(())
}

#[test]
fn test_query_reposts_for_message() -> Result<()> {
    let mut db = setup()?;
    let link = "https://example.com/a";
    let first = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let second = add_msg(&db, 2_000, CHANNEL, SERVER, OTHER_AUTHOR);
    let third = add_msg(&db, 3_000, CHANNEL, SERVER, AUTHOR);
    let other_server = add_msg(&db, 500, OTHER_SERVER_CHANNEL, OTHER_SERVER, AUTHOR);
    for msg in [first, second, third, other_server] {
        db.insert_links([link], msg.id)?;
    }
    let unrelated = add_msg(&db, 4_000, CHANNEL, SERVER, AUTHOR);
    db.insert_links(["https://example.com/b"], unrelated.id)?;

    assert!(db.query_reposts_for_message(first.id)?.is_empty());
    assert_eq!(
        ids(&db.query_reposts_for_message(second.id)?),
        vec![first.id]
    );
    assert_eq!(
        ids(&db.query_reposts_for_message(third.id)?),
        vec![first.id, second.id]
    );
    assert!(db.query_reposts_for_message(unrelated.id)?.is_empty());
    Ok(())
}

#[test]
fn test_hash_matches() -> Result<()> {
    let mut db = setup()?;
    let original = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let different = add_msg(&db, 2_000, CHANNEL, SERVER, AUTHOR);
    let current = add_msg(&db, 3_000, CHANNEL, SERVER, AUTHOR);
    db.insert_images([("https://cdn/a.png", HASH)], original.id)?;
    db.insert_images([("https://cdn/b.png", OTHER_HASH)], different.id)?;
    db.insert_images([("https://cdn/c.png", HASH)], current.id)?;

    let matches = db.hash_matches(HASH, SERVER, current.id)?;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].0, original);
    assert_eq!(matches[0].1, HASH);

    // no matches in a different server
    assert!(db.hash_matches(HASH, OTHER_SERVER, current.id)?.is_empty());
    Ok(())
}

#[test]
fn test_hash_matches_near_match() -> Result<()> {
    let mut db = setup()?;
    let original = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    db.insert_images([("https://cdn/a.png", HASH)], original.id)?;

    // differs in the final sampled character (offset 14) so only 4 of the 5 match
    let mut near = HASH.to_string();
    near.replace_range(14..15, "#");
    let matches = db.hash_matches(&near, SERVER, 0)?;
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].1, HASH);

    // differs in two sampled characters (offsets 0 and 14)
    near.replace_range(0..1, "#");
    assert!(db.hash_matches(&near, SERVER, 0)?.is_empty());
    Ok(())
}

#[test]
fn test_hash_matches_rejects_invalid_hash() -> Result<()> {
    let db = setup()?;
    assert!(db.hash_matches("short", SERVER, 0).is_err());
    Ok(())
}

#[test]
fn test_insert_images_dedupes_per_message() -> Result<()> {
    let mut db = setup()?;
    let msg = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let image = ("https://cdn/a.png", HASH);
    db.insert_images([image, image], msg.id)?;
    db.insert_images([image], msg.id)?;

    let count = |table: &str| -> Result<u64> {
        db.get_connection()
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
    };
    assert_eq!(count("image")?, 1);
    assert_eq!(count("message_image")?, 1);
    Ok(())
}

#[test]
fn test_repost_list_and_top_reposters() -> Result<()> {
    let mut db = setup()?;
    let popular = "https://example.com/popular";
    let once = "https://example.com/once";
    let first = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    db.insert_links([popular, once], first.id)?;
    for ms in [2_000, 3_000] {
        let msg = add_msg(&db, ms, CHANNEL, SERVER, OTHER_AUTHOR);
        db.insert_links([popular], msg.id)?;
    }

    let reposts = db.get_repost_list(SERVER)?;
    assert_eq!(reposts.len(), 1);
    assert_eq!(reposts[0].link, popular);
    assert_eq!(reposts[0].count, 3);

    let reposters = db.get_top_reposters(SERVER)?;
    assert_eq!(reposters.len(), 1);
    assert_eq!(reposters[0].username, "other_author");
    assert_eq!(reposters[0].count, 2);

    assert!(db.get_repost_list(OTHER_SERVER)?.is_empty());
    assert!(db.get_top_reposters(OTHER_SERVER)?.is_empty());
    Ok(())
}

#[test]
fn test_add_user_updates_username() -> Result<()> {
    let db = setup()?;
    db.add_user(AUTHOR, "renamed", false)?;
    let name: String = db.get_connection().query_row(
        "SELECT username FROM user WHERE id = ?1",
        [AUTHOR],
        |r| r.get(0),
    )?;
    assert_eq!(name, "renamed");
    db.add_nickname(AUTHOR, SERVER, "nick")?;
    // adding the same nickname again is ignored rather than an error
    db.add_nickname(AUTHOR, SERVER, "nick")?;
    Ok(())
}

#[test]
fn test_replies() -> Result<()> {
    let db = setup()?;
    let msg = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    assert!(db.get_reply(msg.id)?.is_none());

    db.add_reply(snowflake(2_000), CHANNEL, msg.id)?;
    // adding the same reply twice is a no-op
    db.add_reply(snowflake(2_000), CHANNEL, msg.id)?;
    let reply = db.get_reply(msg.id)?.expect("reply should exist");
    assert_eq!(reply.id, snowflake(2_000));
    assert_eq!(reply.channel, CHANNEL);
    assert_eq!(reply.replied_to, msg.id);
    Ok(())
}

#[test]
fn test_get_newest_unchecked_message() -> Result<()> {
    let db = setup()?;
    assert_eq!(db.get_newest_unchecked_message(SERVER)?, None);

    let older = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let newer = add_msg(&db, 2_000, CHANNEL, SERVER, AUTHOR);
    add_msg(&db, 3_000, HIDDEN_CHANNEL, SERVER, AUTHOR);
    add_msg(&db, 4_000, OTHER_SERVER_CHANNEL, OTHER_SERVER, AUTHOR);

    assert_eq!(db.get_newest_unchecked_message(SERVER)?, Some(newer));

    // a message must be both checked old and parsed to be skipped
    db.mark_message_checked_old(newer.id)?;
    assert_eq!(db.get_newest_unchecked_message(SERVER)?, Some(newer));
    db.mark_message_all_checked(newer.id)?;
    assert_eq!(db.get_newest_unchecked_message(SERVER)?, Some(older));

    db.soft_delete_message(older.id)?;
    assert_eq!(db.get_newest_unchecked_message(SERVER)?, None);
    Ok(())
}

#[test]
fn test_get_message_links() -> Result<()> {
    let mut db = setup()?;
    let msg = add_msg(&db, 1_000, CHANNEL, SERVER, AUTHOR);
    let other = add_msg(&db, 2_000, CHANNEL, SERVER, AUTHOR);
    assert!(db.get_message_links(msg.id)?.is_empty());

    db.insert_links(["https://example.com/a", "https://example.com/b"], msg.id)?;
    db.insert_links(["https://example.com/c"], other.id)?;
    let mut links = db.get_message_links(msg.id)?;
    links.sort_unstable();
    assert_eq!(
        links,
        vec!["https://example.com/a", "https://example.com/b"]
    );
    Ok(())
}
