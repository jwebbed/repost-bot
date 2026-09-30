mod pins;

use crate::structs::reply::{Reply, ReplyType};

use db::structs::{RepostCount, ReposterCount};
use db::{ReadOnlyDb, read_only_db_call};
use log::{error, warn};
use regex::Regex;
use serenity::model::id::GuildId;
use serenity::{model::channel::Message, prelude::*};
use std::fmt::Write;
use std::sync::LazyLock;

static COMMAND_PREFIX_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^!rp(m|b) ").unwrap());

/// Returns the command following the command prefix, or none if the
/// message doesn't start with the command prefix.
pub(super) fn parse_command(content: &str) -> Option<&str> {
    COMMAND_PREFIX_REGEX
        .find(content)
        .map(|prefix| content[prefix.end()..].trim())
}

fn format_repost_list(reposts: &[RepostCount]) -> String {
    reposts
        .iter()
        .fold(String::from("Count | Link"), |mut response, x| {
            let _ = write!(response, "\n{:<9} | <{}>", x.count, x.link);
            response
        })
}

fn format_reposter_list(reposters: &[ReposterCount]) -> String {
    reposters
        .iter()
        .fold(String::from("Username | Count"), |mut response, x| {
            let _ = write!(response, "\n{} | {:<9}", x.username, x.count);
            response
        })
}

fn repost_cnt(msg: &Message, guild: GuildId) -> Reply {
    let reposts = read_only_db_call(|db| db.get_repost_list(guild.get())).unwrap_or_else(|why| {
        error!("failed to load repost list for {guild}: {why:?}");
        Vec::new()
    });
    Reply::new(
        format_repost_list(&reposts),
        ReplyType::Channel(msg.channel_id),
    )
}

fn reposter_cnt(msg: &Message, guild: GuildId) -> Reply {
    let reposters =
        read_only_db_call(|db| db.get_top_reposters(guild.get())).unwrap_or_else(|why| {
            error!("failed to load top reposters for {guild}: {why:?}");
            Vec::new()
        });
    Reply::new(
        format_reposter_list(&reposters),
        ReplyType::Channel(msg.channel_id),
    )
}

pub async fn handle_command(ctx: &Context, msg: &Message, command: &str) -> Option<Reply> {
    let Some(guild) = msg.guild_id else {
        warn!("Ignoring command {command} sent outside of a server");
        return None;
    };

    match command {
        "pins" => match pins::pins(ctx, msg, guild).await {
            Ok(reply) => Some(reply),
            Err(why) => {
                warn!("Failed to process command {command} with err: {why}");
                None
            }
        },
        "reposts" => Some(repost_cnt(msg, guild)),
        "reposters" => Some(reposter_cnt(msg, guild)),
        _ => Some(Reply::new("Unrecognized command", ReplyType::from(msg))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_command_prefix_rpm() {
        assert!(parse_command("!rpm pins").is_some());
        assert!(parse_command("!rpm reposts").is_some());
        assert!(parse_command("!rpm reposters").is_some());
        assert!(parse_command("!rpm wordle score").is_some());
        assert!(parse_command("!rpm wordle server").is_some());
        assert!(parse_command("!rpm allowlist").is_some());
    }

    #[test]
    fn test_command_prefix_rpb() {
        assert!(parse_command("!rpb pins").is_some());
        assert!(parse_command("!rpb reposts").is_some());
        assert!(parse_command("!rpb reposters").is_some());
        assert!(parse_command("!rpb wordle score").is_some());
        assert!(parse_command("!rpb wordle server").is_some());
        assert!(parse_command("!rpb allowlist").is_some());
    }

    #[test]
    fn test_command_prefix_not_start() {
        assert!(parse_command("   !rpb pins").is_none());
    }

    #[test]
    fn test_command_prefix_no_exclaimation() {
        assert!(parse_command("rpb pins").is_none());
    }

    #[test]
    fn test_command_prefix_non_command() {
        assert!(parse_command("").is_none());
        assert!(parse_command("!").is_none());
        assert!(parse_command("hello world!").is_none());
    }

    #[test]
    fn test_parse_command() {
        assert_eq!(parse_command("!rpm pins"), Some("pins"));
        assert_eq!(parse_command("!RPB reposts  "), Some("reposts"));
        assert_eq!(parse_command("!rpm   reposters"), Some("reposters"));
        assert_eq!(parse_command("!rpm "), Some(""));
        assert_eq!(parse_command("!rpmpins"), None);
        // multi-byte characters directly after the prefix are handled
        assert_eq!(parse_command("!rpm 🎉"), Some("🎉"));
    }

    #[test]
    fn test_format_repost_list() {
        assert_eq!(format_repost_list(&[]), "Count | Link");
        let reposts = [
            RepostCount {
                link: "https://a".into(),
                count: 3,
            },
            RepostCount {
                link: "https://b".into(),
                count: 2,
            },
        ];
        assert_eq!(
            format_repost_list(&reposts),
            "Count | Link\n3         | <https://a>\n2         | <https://b>"
        );
    }

    #[test]
    fn test_format_reposter_list() {
        assert_eq!(format_reposter_list(&[]), "Username | Count");
        let reposters = [ReposterCount {
            username: "user".into(),
            count: 4,
        }];
        assert_eq!(
            format_reposter_list(&reposters),
            "Username | Count\nuser | 4        "
        );
    }
}
