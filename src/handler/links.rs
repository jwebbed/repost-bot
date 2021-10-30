use super::{log_error, Handler};

use crate::db::DB;
use crate::errors::Result;
use crate::structs::Link;

use lazy_static::lazy_static;
use linkify::{LinkFinder, LinkKind};
use phf::phf_set;
use regex::Regex;
use serenity::{
    model::channel::Message,
    model::id::{ChannelId, GuildId, MessageId},
    prelude::*,
};
use url::Url;

// largely sourced from newhouse/url-tracking-stripper on github

static TWITTER_FIELDS: phf::Set<&'static str> = phf_set! {
    "s",
    "t"
};

static GENERIC_FIELDS: phf::Set<&'static str> = phf_set! {
    // Google's Urchin Tracking Module
    "utm_source",
    "utm_medium",
    "utm_term",
    "utm_campaign",
    "utm_content",
    "utm_name",
    "utm_cid",
    "utm_reader",
    "utm_viz_id",
    "utm_pubreferrer",
    "utm_swu",
    // Mailchimp
    "mc_cid",
    "mc_eid",
    // comScore Digital Analytix?
    // http://www.about-digitalanalytics.com/comscore-digital-analytix-url-campaign-generator
    "ns_source",
    "ns_mchannel",
    "ns_campaign",
    "ns_linkname",
    "ns_fee",
    // Simple Reach
    "sr_share",
    // Facebook Click Identifier
    // http://thisinterestsme.com/facebook-fbclid-parameter/
    "fbclid",
    // Instagram Share Identifier
    "igshid",
    "srcid",
    // Google Click Identifier
    "gclid",
    // Some other Google Click thing
    "ocid",
    // Unknown
    "ncid",
    // Unknown
    "nr_email_referer",
    // Generic-ish. Facebook, Product Hunt and others
    "ref",
    // Alibaba-family 'super position model' tracker:
    // https://github.com/newhouse/url-tracking-stripper/issues/38
    "spm",
};

/// filter_field returns true if we should filter a field out in a query string,
/// otherwise returns false.
///
/// We filter fields that are largely meant for tracking and as such not meaningfully
/// useful for comparison purposes. Without filtering out tracking filters otherwise
/// identical links may not be the same because of different tracking values for
/// different users.
///
/// Requires the host as well as sometimes we do specific filters for specifics hosts
/// i.e we filter "s" on twitter but nothing else. It should be expected that this
/// function will grow over time
#[inline(always)]
fn filter_field(host: &str, field: &str) -> bool {
    let host_match = match host {
        "twitter" | "twitter.com" => TWITTER_FIELDS.contains(&field),
        _ => false,
    };
    host_match || GENERIC_FIELDS.contains(&field)
}

fn transform_url(url: Url) -> Result<Url> {
    let ret = if url.host_str().is_some() {
        match url.host_str().unwrap() {
            "youtu.be" => {
                let path = url.path();
                if path.len() > 1 {
                    let expanded_url =
                        format!("https://www.youtube.com/watch?v={}", &path[1..path.len()]);
                    Some(Url::parse(&expanded_url)?)
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    };

    match ret {
        Some(value) => Ok(value),
        None => Ok(url),
    }
}

/// filtered_url takes a url_str and returns a Url object with the any irrelevent
/// fields in the query string removed as per filter_field
fn filtered_url(url_str: &str) -> Result<Url> {
    let mut url = transform_url(Url::parse(url_str)?)?;
    let host = url.host_str().ok_or(rusqlite::Error::QueryReturnedNoRows)?;

    let fields = url
        .query_pairs()
        .filter(|(field, _value)| !filter_field(host, &field))
        .map(|(f, v)| (f.into_owned(), v.into_owned()))
        .collect::<Vec<(String, String)>>();

    let mut query = url.query_pairs_mut();
    query.clear();
    for field in fields {
        query.append_pair(&field.0, &field.1);
    }

    // need this to ensure no dangling references
    drop(query);

    // if query is some(empty string) then the result will contain a dangling ?, this removes that
    if url.query() == Some("") {
        url.set_query(None);
    }

    println!("Filtered url: {:?}", url);
    Ok(url)
}

fn query_link_matches(url_str: &str, server: u64) -> Result<Vec<Link>> {
    let mut links = Vec::new();
    for link in DB::db_call(|db| db.query_links(url_str, server))? {
        links.push(link)
    }
    Ok(links)
}

fn get_link_str(link: &Link) -> String {
    MessageId(link.message).link(ChannelId(link.channel), Some(GuildId(link.server)))
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

            // first need to insert into db
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
        reposts: &Vec<Link>,
    ) -> Result<()> {
        if reposts.len() > 0 {
            let repost_str = if reposts.len() > 1 {
                format!(
                    "\n{}",
                    reposts
                        .into_iter()
                        .map(|x| get_link_str(&x))
                        .collect::<Vec<String>>()
                        .join("\n")
                )
            } else {
                get_link_str(&reposts[0])
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

    #[test]
    fn test_filter_link() -> Result<()> {
        assert_eq!(filter_field("www.youtube.com", "v"), false);
        assert_eq!(filter_field("twitter.com", "s"), true);

        let filtered = filtered_url("https://twitter.com/user/status/idnumber?s=21")?;
        assert_eq!(
            filtered.as_str(),
            "https://twitter.com/user/status/idnumber"
        );

        Ok(())
    }

    #[test]
    fn test_youtube_sl() -> Result<()> {
        let url = Url::parse("https://youtu.be/fakeid")?;
        assert_eq!(
            transform_url(url)?.as_str(),
            "https://www.youtube.com/watch?v=fakeid"
        );
        Ok(())
    }
}
