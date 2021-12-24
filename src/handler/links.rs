use super::filter::filtered_url;
use super::{log_error, Handler};

use crate::db::DB;
use crate::errors::{Error, Result};
use crate::structs::Link;

use humantime::format_duration;
use lazy_static::lazy_static;
use linkify::{LinkFinder, LinkKind};
use regex::Regex;
use serenity::{model::channel::Message, prelude::*};
use std::time::Duration;

fn query_link_matches(url_str: &str, server: u64) -> Result<Vec<Link>> {
    let mut links = Vec::new();
    for link in DB::db_call(|db| db.query_links(url_str, server))? {
        links.push(link)
    }
    Ok(links)
}

// returns true if the input string is a discord message link
fn is_discord_link(text: &str) -> bool {
    lazy_static! {
        static ref RE: Regex =
            Regex::new(r"^https?://(discord\.com/channels|tenor\.com/view)/\S*").unwrap();
    }
    RE.is_match(text)
}

fn get_links(msg: &str) -> Vec<String> {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder
        .links(msg)
        .map(|x| x.as_str().to_string())
        .filter(|link| !is_discord_link(link))
        .collect()
}

fn get_duration(msg: &Message, link: &Link) -> Result<Duration> {
    let ret = msg
        .id
        .created_at()
        .signed_duration_since(link.created_at)
        .to_std();
    match ret {
        Ok(val) => Ok(val),
        Err(why) => {
            println!("Failed to get duration with following error: {:?}", why);
            Err(Error::Internal(format!("{:?}", why)))
        }
    }
}

fn repost_text(msg: &Message, link: &Link) -> String {
    let duration_text = match get_duration(msg, link) {
        Ok(val) => format_duration(val).to_string(),
        Err(_) => "".to_string(),
    };

    format!("{} {}", duration_text, link.message_uri())
}

impl Handler {
    pub fn store_links_and_get_reposts(&self, msg: &Message) -> Vec<Link> {
        let mut reposts = Vec::new();
        let server_id = *msg.guild_id.unwrap().as_u64();
        for link in get_links(&msg.content) {
            let filtered_link = match filtered_url(&link) {
                Ok(url) => url,
                Err(why) => {
                    println!("Failed to filter url: {:?}", why);
                    continue;
                }
            };
            match query_link_matches(filtered_link.as_str(), server_id) {
                Ok(results) => {
                    println!("Found {} reposts: {:?}", results.len(), results);
                    for result in results {
                        reposts.push(result);
                    }
                }
                Err(why) => {
                    println!("Failed to load reposts with err: {:?}", why);
                }
            };

            // finally insert this link into db
            log_error(
                DB::db_call(|db| db.insert_link(filtered_link.as_str(), *msg.id.as_u64())),
                "Insert link",
            );
        }

        reposts
    }

    async fn reply_with_reposts(
        &self,
        ctx: &Context,
        msg: &Message,
        reposts: &[Link],
    ) -> Result<()> {
        if !reposts.is_empty() {
            let repost_str = if reposts.len() > 1 {
                format!(
                    "\n{}",
                    reposts
                        .iter()
                        .map(|x| repost_text(msg, x))
                        .collect::<Vec<String>>()
                        .join("\n")
                )
            } else {
                repost_text(msg, &reposts[0])
            };

            msg.reply(&ctx.http, format!("🚨 REPOST 🚨 {}", repost_str))
                .await?;
        }
        Ok(())
    }
    pub async fn check_links(&self, ctx: &Context, msg: &Message) {
        let reposts = self.store_links_and_get_reposts(msg);
        match self.reply_with_reposts(ctx, msg, &reposts).await {
            Ok(_) => (),
            Err(why) => println!("Failed to inform of REPOST: {:?}", why),
        }
    }
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
}
