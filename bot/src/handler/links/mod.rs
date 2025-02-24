mod filter;

use crate::errors::Result;
use crate::structs::repost::{RepostSet, RepostType};
use crate::structs::{Post, PostProcessor, ProcessedPost};
use filter::filtered_url;

use db::{get_read_only_db, read_only_db_call, writable_db_call, ReadOnlyDb, WriteableDb};
use lazy_static::lazy_static;
use linkify::{LinkFinder, LinkKind};
use log::{error, info};
use regex::Regex;
use url::Url;

const IGNORED_DOMAINS: [&str; 5] = [
    r"globle-game\.com",
    r"discord\.com/channels",
    r"tenor\.com/view",
    r"heardle\.app",
    r"worldle\.teuteuf\.fr",
];

/// returns true if the input link is one of the ignored domains
fn ignored_domain(text: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(format!(r"https?://({})/?\S*", IGNORED_DOMAINS.join("|")).as_str()).unwrap();
    }
    RE.is_match(text)
}

#[derive(Debug)]
pub struct LinkProcessor {
    post: Post,
}

struct Links {
    msg_id: u64,
    server_id: u64,
    links: Box<[Url]>,
}

impl PostProcessor for LinkProcessor {
    fn new(post: Post) -> LinkProcessor {
        LinkProcessor { post }
    }

    async fn process(&self) -> Result<impl ProcessedPost> {
        let links = get_links(self.post.content())
            .map(|link| filtered_url(&link))
            .filter_map(|url| match url {
                Ok(url) => Some(url),
                Err(why) => {
                    error!("Failed to filter url: {why:?}");
                    None
                }
            })
            .collect();

        Ok(Links {
            msg_id: self.post.id(),
            server_id: self.post.server_id(),
            links,
        })
    }
}

impl ProcessedPost for Links {
    fn get_reposts(&self) -> Result<RepostSet> {
        let mut reposts = RepostSet::new();
        if !self.links.is_empty() {
            let db = get_read_only_db()?;
            for link in &self.links {
                for rlink in db.query_links(link.as_str(), self.server_id)? {
                    reposts.add(rlink.message, RepostType::Link);
                }
            }
        }
        if reposts.len() > 0 {
            info!("Found {} reposts: {reposts:?}", reposts.len());
        }

        Ok(reposts)
    }

    fn store_post(&self) -> Result<()> {
        for link in &self.links {
            writable_db_call(|mut db| db.insert_link(link.as_str(), self.msg_id))?;
        }
        Ok(())
    }
}

fn get_links(msg: &str) -> impl Iterator<Item = Box<str>> + use<'_> {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder
        .links(msg)
        .filter(|link| !ignored_domain(link.as_str()))
        .map(|x| x.as_str().into())
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
        assert_eq!(
            links[0],
            "https://twitter.com/user/status/idnumber?s=20".into()
        );
    }

    #[test]
    fn test_extract_multiple_links() {
        let links = get_links(
            "test msg with link https://twitter.com/user/status/idnumber?s=20 and
             another link https://www.bbc.com/news/article",
        )
        .collect::<Vec<_>>();

        assert_eq!(links.len(), 2);
        assert!(links.contains(&"https://twitter.com/user/status/idnumber?s=20".into()));
        assert!(links.contains(&"https://www.bbc.com/news/article".into()));
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
        assert!(links.contains(&"https://www.bbc.com/news/article".into()));
        assert!(links.contains(&"https://discord.com/developers/docs/intro".into()));
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
}
