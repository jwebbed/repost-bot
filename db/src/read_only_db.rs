use crate::connections::GetConnectionImmutable;
use crate::queries;
use crate::structs::{Channel, Link, Message, Reply, RepostCount, ReposterCount};

use rusqlite::{OptionalExtension, Result, Row};
use serenity::model::id::{ChannelId, GuildId, MessageId};

#[inline(always)]
fn extract_first_result<I, T>(iter: &mut I) -> Result<Option<T>>
where
    I: Iterator<Item = rusqlite::Result<T>>,
{
    // I hate this less than what I was doing before, but still works
    let ret = match iter.next() {
        Some(ret) => Some(ret?),
        None => None,
    };

    Ok(ret)
}

pub trait ReadOnlyDb: GetConnectionImmutable {
    #[inline]
    fn get_message(&self, message_id: MessageId) -> Result<Option<Message>> {
        queries::get_message(self.get_connection(), *message_id.as_u64())
    }

    #[inline]
    fn get_newest_unchecked_message(&self, server_id: u64) -> Result<Option<Message>> {
        let mut stmt = self.get_connection().prepare(
            "SELECT M.id, M.server, M.channel, M.author, M.created_at, 
                    M.parsed_repost, M.deleted, M.checked_old, M.parsed_embed
            FROM message as M 
            JOIN channel as C on C.id=M.channel
            WHERE 
                C.visible=TRUE AND
                M.deleted IS NULL AND
                M.server=(?1) AND 
                ( M.checked_old IS NULL OR
                  M.parsed_embed IS NULL )
            ORDER BY M.created_at desc
            LIMIT 1",
        )?;
        let mut rows = stmt.query_map([server_id], |row| {
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
        })?;
        extract_first_result(&mut rows)
    }

    #[inline]
    fn get_known_channels(&self, server_id: u64) -> Result<Vec<Channel>> {
        let mut stmt = self.get_connection().prepare(
            "SELECT * FROM channel 
            WHERE server=(?1) AND 
                visible=TRUE",
        )?;

        let rows = stmt.query_map([server_id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                name: row.get(1)?,
                visible: row.get(2)?,
                server: row.get(3)?,
            })
        })?;

        let mut channels = Vec::new();
        for row in rows {
            channels.push(row?)
        }
        Ok(channels)
    }

    #[inline]
    fn get_channel_list(&self, server_id: GuildId) -> Result<Vec<(ChannelId, String)>> {
        let mut stmt = self
            .get_connection()
            .prepare("SELECT id, name FROM channel where server = (?1)")?;
        let rows = stmt.query_map([*server_id.as_u64()], |row| {
            Ok((ChannelId(row.get(0)?), row.get(1)?))
        })?;
        let mut links = Vec::new();
        for row in rows {
            links.push(row?)
        }
        Ok(links)
    }

    #[inline]
    fn query_links(&self, link: &str, server: u64) -> Result<Vec<Link>> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare(
            "SELECT 
                L.id, L.link, S.id, C.id, M.id, M.created_at, C.name, 
                S.name, M.author, M.parsed_repost, 
                M.deleted, M.checked_old, M.parsed_embed
            FROM link AS L 
            JOIN message_link as ML on ML.link=L.id
            JOIN message as M on ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            WHERE 
                L.link = (?1)
                AND S.id = (?2)
                AND C.visible = TRUE
                AND M.deleted IS NULL;",
        )?;
        let rows = stmt.query_map((link, server), |row| {
            Ok(Link {
                id: row.get(0)?,
                link: row.get(1)?,
                channel_name: row.get(6)?,
                server_name: row.get(7)?,
                message: Message::new(
                    row.get(4)?,  // id
                    row.get(2)?,  // server
                    row.get(3)?,  // channel
                    row.get(8)?,  // author
                    row.get(5)?,  // created_at
                    row.get(9)?,  // parsed_repost
                    row.get(12)?, // parsed_embed
                    row.get(10)?, // deleted
                    row.get(11)?, // checked_old
                ),
            })
        })?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row?)
        }

        Ok(links)
    }

    #[inline]
    fn query_reposts_for_message(&self, message_id: u64) -> Result<Vec<Message>> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare(
            "SELECT 
                MR.id, MR.server, MR.channel, MR.author, MR.created_at, 
                MR.parsed_repost, MR.deleted, MR.checked_old, MR.parsed_embed
            FROM message_link AS ML
            JOIN message AS M ON ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            JOIN message_link AS MLR ON ML.link=MLR.link
            JOIN message AS MR ON MLR.message = MR.id
            WHERE 
                M.id = (?1)
                AND C.visible = TRUE
                AND M.deleted IS NULL
                AND M.server == MR.server
                AND ML.id != MLR.id
                AND MR.created_at < M.created_at",
        )?;
        let rows = stmt.query_map([message_id], |row| {
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
        })?;
        let mut posts = Vec::new();
        for row in rows {
            posts.push(row?);
        }
        Ok(posts)
    }

    #[inline]
    fn hash_matches(
        &self,
        hash: &str,
        server: u64,
        current_msg_id: u64,
    ) -> Result<Vec<(Message, String)>> {
        let conn = self.get_connection();

        let mut stmt = conn.prepare(
            "SELECT M.id, M.server, M.channel, M.author, M.created_at, 
            M.parsed_repost, M.deleted, M.checked_old, M.parsed_embed, I.hash
            FROM image as I
            JOIN message_image as MI on MI.image=I.id
            JOIN message as M on M.id=MI.message
            JOIN server AS S ON M.server=S.id
            JOIN channel AS C ON M.channel=C.id
            WHERE 
            (   I.hash = (?6) OR
                ( I.c1 = (?1) AND I.c2 = (?2) AND I.c3 = (?3) AND I.c4 = (?4) ) OR
                ( I.c2 = (?2) AND I.c3 = (?3) AND I.c4 = (?4) AND I.c5 = (?5) ) OR
                ( I.c3 = (?3) AND I.c4 = (?4) AND I.c5 = (?5) AND I.c1 = (?1) ) OR
                ( I.c4 = (?4) AND I.c5 = (?5) AND I.c1 = (?1) AND I.c2 = (?2) ) OR
                ( I.c5 = (?5) AND I.c1 = (?1) AND I.c2 = (?2) AND I.c3 = (?3) )
            )
            AND S.id = (?7)
            AND M.id != (?8)
            AND C.visible = TRUE
            AND M.deleted IS NULL",
        )?;
        assert!(hash.len() >= 5);
        let mut chars = hash.chars();
        let rows = stmt.query_map(
            (
                String::from(chars.next().unwrap()),
                String::from(chars.nth(1).unwrap()),
                String::from(chars.nth(2).unwrap()),
                String::from(chars.nth(3).unwrap()),
                String::from(chars.nth(4).unwrap()),
                hash,
                server,
                current_msg_id
            ),
            |row: &Row<'_>| -> rusqlite::Result<(Message, String)> {
                Ok((
                    Message::new(
                        row.get(0)?, // id
                        row.get(1)?, // server
                        row.get(2)?, // channel
                        row.get(3)?, // author
                        row.get(4)?, // created_at
                        row.get(5)?, // parsed_repost
                        row.get(8)?, // parsed_embed
                        row.get(6)?, // deleted
                        row.get(7)?, // checked_old
                    ),
                    row.get(9)?,
                ))
            },
        )?;

        let mut links = Vec::new();
        for row in rows {
            links.push(row?)
        }

        Ok(links)
    }

    #[inline]
    fn get_repost_list(&self, server_id: u64) -> Result<Vec<RepostCount>> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare(
            "SELECT L.link, LM.link_count 
            FROM link as L JOIN (
                SELECT 
                    ML.link, 
                    COUNT(1) as link_count,
                    MAX(M.created_at) as most_recent 
                FROM message_link as ML 
                JOIN message as M on ML.message=M.id
                JOIN channel as C on M.channel=C.id
                WHERE M.server=(?1) AND 
                    C.visible=TRUE AND
                    M.deleted IS NULL
                GROUP BY link
                HAVING link_count > 1 
            ) as LM on L.id=LM.link
            ORDER BY link_count desc, most_recent desc
            LIMIT 10",
        )?;

        let rows = stmt.query_map([server_id], |row| {
            Ok(RepostCount {
                link: row.get(0)?,
                count: row.get(1)?,
            })
        })?;

        let mut reposts = Vec::new();
        for repost in rows {
            reposts.push(repost?)
        }

        Ok(reposts)
    }

    #[inline]
    fn get_top_reposters(&self, server_id: u64) -> Result<Vec<ReposterCount>> {
        let conn = self.get_connection();
        let mut stmt = conn.prepare(
            "SELECT U.username, COUNT(*) as cnt
            FROM message_link AS L1
            JOIN (
                SELECT MIN(ML.id) as id, ML.link
                FROM message_link as ML
                JOIN message as M on M.id = ML.message 
                JOIN channel as C on M.channel=C.id
                WHERE M.server=(?1) AND 
                    C.visible=TRUE AND
                    M.deleted IS NULL
                GROUP BY link
            ) as L2 on L1.link=L2.link
            JOIN message as M on M.id=L1.message
            JOIN user as U on M.author=U.id
            WHERE L1.id != L2.id
            GROUP BY U.username
            ORDER BY cnt desc",
        )?;

        let rows = stmt.query_map([server_id], |row| {
            Ok(ReposterCount {
                username: row.get(0)?,
                count: row.get(1)?,
            })
        })?;

        let mut reposters = Vec::new();
        for repost in rows {
            reposters.push(repost?)
        }

        Ok(reposters)
    }

    #[inline]
    fn get_reply(&self, replied_id: u64) -> Result<Option<Reply>> {
        let conn = self.get_connection();
        conn.query_row(
            "SELECT id, channel, replied_to
            FROM reply WHERE replied_to=(?1)",
            [replied_id],
            |row| {
                Ok(Reply {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    replied_to: row.get(2)?,
                })
            },
        )
        .optional()
    }
}
