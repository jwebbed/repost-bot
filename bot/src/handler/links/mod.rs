mod filter;

use crate::errors::Result;
use crate::structs::repost::{RepostSet, RepostType};
use crate::structs::{Post, PostProcessor, ProcessedPost};
pub(super) use filter::{filtered_url, site_host};

use db::{ReadOnlyDb, WriteableDb, get_read_only_db, read_only_db_call, writable_db_call};
use linkify::{LinkFinder, LinkKind};
use log::{error, info};
use regex::Regex;
use std::sync::LazyLock;
use url::Url;

const IGNORED_DOMAINS: [&str; 5] = [
    r"globle-game\.com",
    r"discord\.com/channels",
    r"tenor\.com/view",
    r"heardle\.app",
    r"worldle\.teuteuf\.fr",
];

static IGNORED_DOMAIN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(&format!(
        r"^https?://({})([/?#]|$)",
        IGNORED_DOMAINS.join("|")
    ))
    .unwrap()
});

/// returns true if the input link is one of the ignored domains
fn ignored_domain(text: &str) -> bool {
    IGNORED_DOMAIN_REGEX.is_match(text)
}

#[derive(Debug)]
pub struct LinkProcessor;

pub struct Links {
    msg_id: u64,
    server_id: u64,
    links: Vec<Url>,
}

impl PostProcessor for LinkProcessor {
    type Processed = Links;

    async fn process(post: &Post) -> Result<Links> {
        Ok(Links {
            msg_id: post.id(),
            server_id: post.server_id(),
            links: normalized_links(post.content()),
        })
    }
}

impl ProcessedPost for Links {
    fn get_reposts(&self) -> Result<RepostSet> {
        let mut reposts = RepostSet::default();
        if !self.links.is_empty() {
            let db = get_read_only_db()?;
            for link in &self.links {
                for rlink in db.query_links(link.as_str(), self.server_id)? {
                    reposts.add(rlink.message, RepostType::Link);
                }
            }
        }
        if !reposts.is_empty() {
            info!("Found {} reposts: {reposts:?}", reposts.len());
        }

        Ok(reposts)
    }

    fn store_post(&self) -> Result<()> {
        if !self.links.is_empty() {
            writable_db_call(|mut db| {
                db.insert_links(self.links.iter().map(Url::as_str), self.msg_id)
            })?;
        }
        Ok(())
    }
}

/// Returns the deduplicated links in a message, with tracking fields removed
/// and hosts normalised, in the form they are stored in the db
pub(super) fn normalized_links(content: &str) -> Vec<Url> {
    let mut links: Vec<Url> = get_links(content)
        .filter_map(|link| match filtered_url(link) {
            Ok(url) => Some(url),
            Err(why) => {
                error!("Failed to filter url {link}: {why:?}");
                None
            }
        })
        .collect();
    // the same link posted twice in one message is still one post
    links.sort_unstable();
    links.dedup();
    links
}

fn get_links(msg: &str) -> impl Iterator<Item = &str> {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder
        .links(msg)
        .map(|link| link.as_str())
        .filter(|link| !ignored_domain(link))
}

pub fn get_reposts_for_message_id(message_id: u64) -> Result<RepostSet> {
    Ok(RepostSet::new_from_messages(
        &read_only_db_call(|db| db.query_reposts_for_message(message_id))?,
        RepostType::Link,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_extract_link() {
        let links = get_links("test msg with link https://twitter.com/user/status/idnumber?s=20")
            .collect::<Vec<_>>();

        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "https://twitter.com/user/status/idnumber?s=20");
    }

    #[test]
    fn test_extract_multiple_links() {
        let links = get_links(
            "test msg with link https://twitter.com/user/status/idnumber?s=20 and
             another link https://www.bbc.com/news/article",
        )
        .collect::<Vec<_>>();

        assert_eq!(links.len(), 2);
        assert!(links.contains(&"https://twitter.com/user/status/idnumber?s=20"));
        assert!(links.contains(&"https://www.bbc.com/news/article"));
    }

    #[test]
    fn test_ignore_discord_links() {
        let links = get_links(
            "test msg with link https://discord.com/channels/guild/channel/msg and
            without the https http://discord.com/channels/guild/channel/msg
            and also ignore tenor https://tenor.com/view/gif-name
             another link https://www.bbc.com/news/article
             discord link but not a channel https://discord.com/developers/docs/intro",
        )
        .collect::<Vec<_>>();

        assert_eq!(links.len(), 2);
        assert!(links.contains(&"https://www.bbc.com/news/article"));
        assert!(links.contains(&"https://discord.com/developers/docs/intro"));
    }

    #[test]
    fn test_extract_no_link() {
        assert_eq!(
            get_links("just a random message with no links in it")
                .collect::<Vec<_>>()
                .len(),
            0
        );
        assert_eq!(
            get_links("example@example.org isnt a link but could be by some definitions")
                .collect::<Vec<_>>()
                .len(),
            0
        );
    }

    #[test]
    fn test_ignore_globle() {
        let message = r"🌎 Feb 26, 2022 🌍
        Today's guesses: 17
        Current streak: 2
        Average guesses: 16.5
        
        https://globle-game.com/";

        assert_eq!(get_links(message).collect::<Vec<_>>().len(), 0);
        // Also assert with no trailing slash
        assert_eq!(
            get_links("https://globle-game.com")
                .collect::<Vec<_>>()
                .len(),
            0
        );
    }

    #[test]
    fn test_ignore_heardle() {
        let message = r"#Heardle #27

        🔈⬛️⬛️⬛️⬛️⬛️🟩
        
        https://heardle.app/";

        assert_eq!(get_links(message).collect::<Vec<_>>().len(), 0);
        // Also assert with no trailing slash
        assert_eq!(
            get_links("https://heardle.app").collect::<Vec<_>>().len(),
            0
        );
    }

    #[test]
    fn test_ignore_worldle() {
        let message = r"#Worldle #65 3/6 (100%)
        🟩🟨⬛⬛⬛↘️
        🟩🟩🟩⬛⬛↙️
        🟩🟩🟩🟩🟩🎉
        https://worldle.teuteuf.fr/";

        assert_eq!(get_links(message).collect::<Vec<_>>().len(), 0);
        // Also assert with no trailing slash
        assert_eq!(
            get_links("https://worldle.teuteuf.fr")
                .collect::<Vec<_>>()
                .len(),
            0
        );
    }

    #[test]
    fn test_ignored_domain_only_matches_host() {
        assert!(ignored_domain("https://tenor.com/view/some-gif"));
        assert!(ignored_domain("http://heardle.app"));
        // the ignored domain appearing elsewhere in the link doesn't count
        assert!(!ignored_domain(
            "https://example.com/?next=https://tenor.com/view/some-gif"
        ));
        // nor does a domain that merely starts with an ignored domain
        assert!(!ignored_domain("https://heardle.application.com/"));
    }

    #[tokio::test]
    async fn test_process_filters_and_dedupes_links() {
        let db_message =
            db::structs::Message::new(1, 2, 3, None, chrono::Utc::now(), None, None, None, None);
        let msg: serenity::model::channel::Message = serde_json::from_value(serde_json::json!({
            "id": "1",
            "channel_id": "3",
            "author": { "id": "4", "username": "user", "discriminator": "0000", "avatar": null },
            "content": "https://x.com/User/status/1?s=20 https://twitter.com/user/status/1 \
                        https://tenor.com/view/gif https://example.com/?utm_source=a&b=c",
            "timestamp": "2024-01-01T00:00:00Z",
            "edited_timestamp": null,
            "tts": false,
            "mention_everyone": false,
            "mentions": [],
            "mention_roles": [],
            "attachments": [],
            "embeds": [],
            "pinned": false,
            "type": 0,
        }))
        .unwrap();
        let post = Post::from_message(&db_message, &msg);
        let links = LinkProcessor::process(&post).await.unwrap();

        assert_eq!(links.msg_id, 1);
        assert_eq!(links.server_id, 2);
        let links: Vec<&str> = links.links.iter().map(Url::as_str).collect();
        assert_eq!(
            links,
            vec![
                "https://example.com/?b=c",
                "https://twitter.com/user/status/1"
            ]
        );
    }
}
