use crate::connections::GetConnectionImmutable;
use crate::queries::{self, MESSAGE_COLUMN_COUNT, hash_samples, message_columns, message_from_row};
use crate::structs::{Channel, Link, Message, Reply, RepostCount, ReposterCount};

use rusqlite::{OptionalExtension, Result};

pub trait ReadOnlyDb: GetConnectionImmutable {
    #[inline]
    fn get_message(&self, message_id: u64) -> Result<Option<Message>> {
        queries::get_message(self.get_connection(), message_id)
    }

    #[inline]
    fn get_newest_unchecked_message(&self, server_id: u64) -> Result<Option<Message>> {
        self.get_connection()
            .prepare_cached(concat!(
                "SELECT ",
                message_columns!("M"),
                " FROM message as M
                JOIN channel as C on C.id=M.channel
                WHERE
                    C.visible=TRUE AND
                    M.deleted IS NULL AND
                    M.server=(?1) AND
                    ( M.checked_old IS NULL OR
                      M.parsed_embed IS NULL )
                ORDER BY M.created_at desc
                LIMIT 1"
            ))?
            .query_row([server_id], |row| message_from_row(row, 0))
            .optional()
    }

    #[inline]
    fn get_known_channels(&self, server_id: u64) -> Result<Vec<Channel>> {
        let mut stmt = self.get_connection().prepare_cached(
            "SELECT id, name, visible, server FROM channel
            WHERE server=(?1) AND
                visible=TRUE",
        )?;

        stmt.query_map([server_id], |row| {
            Ok(Channel {
                id: row.get(0)?,
                name: row.get(1)?,
                visible: row.get(2)?,
                server: row.get(3)?,
            })
        })?
        .collect()
    }

    #[inline]
    fn get_channel_list(&self, server_id: u64) -> Result<Vec<(u64, String)>> {
        let mut stmt = self
            .get_connection()
            .prepare_cached("SELECT id, name FROM channel where server = (?1)")?;
        stmt.query_map([server_id], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect()
    }

    #[inline]
    fn query_links(&self, link: &str, server: u64) -> Result<Vec<Link>> {
        let mut stmt = self.get_connection().prepare_cached(concat!(
            "SELECT L.id, L.link, C.name, S.name, ",
            message_columns!("M"),
            " FROM link AS L
            JOIN message_link as ML on ML.link=L.id
            JOIN message as M on ML.message=M.id
            JOIN channel AS C ON M.channel=C.id
            JOIN server AS S ON M.server=S.id
            WHERE
                L.link = (?1)
                AND S.id = (?2)
                AND C.visible = TRUE
                AND M.deleted IS NULL;"
        ))?;
        stmt.query_map((link, server), |row| {
            Ok(Link {
                id: row.get(0)?,
                link: row.get(1)?,
                channel_name: row.get(2)?,
                server_name: row.get(3)?,
                message: message_from_row(row, 4)?,
            })
        })?
        .collect()
    }

    #[inline]
    fn query_reposts_for_message(&self, message_id: u64) -> Result<Vec<Message>> {
        let mut stmt = self.get_connection().prepare_cached(concat!(
            "SELECT ",
            message_columns!("MR"),
            " FROM message_link AS ML
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
                AND MR.created_at < M.created_at"
        ))?;
        stmt.query_map([message_id], |row| message_from_row(row, 0))?
            .collect()
    }

    /// Returns messages in `server` (other than `current_msg_id`) whose image
    /// hash exactly matches `hash` or shares at least 4 of the 5 sampled
    /// characters. Callers are expected to compute the real distance.
    #[inline]
    fn hash_matches(
        &self,
        hash: &str,
        server: u64,
        current_msg_id: u64,
    ) -> Result<Vec<(Message, String)>> {
        let [c1, c2, c3, c4, c5] = hash_samples(hash)?;
        let mut stmt = self.get_connection().prepare_cached(concat!(
            "SELECT ",
            message_columns!("M"),
            ", I.hash
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
            AND M.deleted IS NULL"
        ))?;
        stmt.query_map((c1, c2, c3, c4, c5, hash, server, current_msg_id), |row| {
            Ok((message_from_row(row, 0)?, row.get(MESSAGE_COLUMN_COUNT)?))
        })?
        .collect()
    }

    #[inline]
    fn get_repost_list(&self, server_id: u64) -> Result<Vec<RepostCount>> {
        let mut stmt = self.get_connection().prepare_cached(
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

        stmt.query_map([server_id], |row| {
            Ok(RepostCount {
                link: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect()
    }

    #[inline]
    fn get_top_reposters(&self, server_id: u64) -> Result<Vec<ReposterCount>> {
        let mut stmt = self.get_connection().prepare_cached(
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

        stmt.query_map([server_id], |row| {
            Ok(ReposterCount {
                username: row.get(0)?,
                count: row.get(1)?,
            })
        })?
        .collect()
    }

    #[inline]
    fn get_reply(&self, replied_id: u64) -> Result<Option<Reply>> {
        self.get_connection()
            .prepare_cached(
                "SELECT id, channel, replied_to
                FROM reply WHERE replied_to=(?1)",
            )?
            .query_row([replied_id], |row| {
                Ok(Reply {
                    id: row.get(0)?,
                    channel: row.get(1)?,
                    replied_to: row.get(2)?,
                })
            })
            .optional()
    }
}
