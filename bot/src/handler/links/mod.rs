mod filter;

use filter::filtered_url;

use crate::errors::Result;
use crate::structs::repost::{RepostSet, RepostType};

use db::structs::Link;
use db::DB;
use lazy_static::lazy_static;
use linkify::{LinkFinder, LinkKind};
use log::{error, info};
use regex::Regex;
use serenity::model::channel::Message;

fn query_link_matches(url_str: &str, server: u64) -> Result<Vec<Link>> {
    let mut links = Vec::new();
    for link in DB::db_call(|db| db.query_links(url_str, server))? {
        links.push(link)
    }
    Ok(links)
}

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

fn get_links(msg: &str) -> Vec<String> {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder
        .links(msg)
        .map(|x| x.as_str().to_string())
        .filter(|link| !ignored_domain(link))
        .collect()
}

pub fn store_links_and_get_reposts(msg: &Message, include_reply: bool) -> Result<RepostSet> {
    let mut reposts = RepostSet::new();
    let server_id = *msg.guild_id.unwrap().as_u64();
    for link in get_links(&msg.content) {
        let filtered_link = match filtered_url(&link) {
            Ok(url) => url,
            Err(why) => {
                error!("Failed to filter url: {why:?}");
                continue;
            }
        };

        if include_reply {
            let repost_links = query_link_matches(filtered_link.as_str(), server_id)?;
            for rlink in repost_links {
                reposts.add(rlink.message, RepostType::Link);
            }
        }

        // finally insert this link into db
        DB::db_call(|db| db.insert_link(filtered_link.as_str(), *msg.id.as_u64()))?;
    }
    // if include_reply false len should always be 0
    if reposts.len() > 0 {
        info!("Found {} reposts: {reposts:?}", reposts.len());
    }
    Ok(reposts)
}

pub fn get_reposts_for_message_id(message_id: u64) -> Result<RepostSet> {
    Ok(RepostSet::new_from_messages(
        &DB::db_call(|db| db.query_reposts_for_message(message_id))?,
        RepostType::Link,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_extract_link() {
        let links = get_links("test msg with link https://twitter.com/user/status/idnumber?s=20");

        assert_eq!(links.len(), 1);
        assert_eq!(links[0], "https://twitter.com/user/status/idnumber?s=20");
    }

    #[test]
    fn test_extract_multiple_links() {
        let links = get_links(
            "test msg with link https://twitter.com/user/status/idnumber?s=20 and
             another link https://www.bbc.com/news/article",
        );

        assert_eq!(links.len(), 2);
        assert!(links.contains(&"https://twitter.com/user/status/idnumber?s=20".to_string()));
        assert!(links.contains(&"https://www.bbc.com/news/article".to_string()));
    }

    #[test]
    fn test_ignore_discord_links() {
        let links = get_links(
            "test msg with link https://discord.com/channels/guild/channel/msg and
            without the https http://discord.com/channels/guild/channel/msg
            and also ignore tenor https://tenor.com/view/gif-name
             another link https://www.bbc.com/news/article
             discord link but not a channel https://discord.com/developers/docs/intro",
        );

        assert_eq!(links.len(), 2);
        assert!(links.contains(&"https://www.bbc.com/news/article".to_string()));
        assert!(links.contains(&"https://discord.com/developers/docs/intro".to_string()));
    }

    #[test]
    fn test_extract_no_link() {
        assert_eq!(
            get_links("just a random message with no links in it").len(),
            0
        );
        assert_eq!(
            get_links("example@example.org isnt a link but could be by some definitions").len(),
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

        assert_eq!(get_links(message).len(), 0);
        // Also assert with no trailing slash
        assert_eq!(get_links("https://globle-game.com").len(), 0);
    }

    #[test]
    fn test_ignore_heardle() {
        let message = r"#Heardle #27

        🔈⬛️⬛️⬛️⬛️⬛️🟩
        
        https://heardle.app/";

        assert_eq!(get_links(message).len(), 0);
        // Also assert with no trailing slash
        assert_eq!(get_links("https://heardle.app").len(), 0);
    }

    #[test]
    fn test_ignore_worldle() {
        let message = r"#Worldle #65 3/6 (100%)
        🟩🟨⬛⬛⬛↘️
        🟩🟩🟩⬛⬛↙️
        🟩🟩🟩🟩🟩🎉
        https://worldle.teuteuf.fr/";

        assert_eq!(get_links(message).len(), 0);
        // Also assert with no trailing slash
        assert_eq!(get_links("https://worldle.teuteuf.fr").len(), 0);
    }
}
