use super::{log_error, Handler};

use crate::db::DB;
use crate::structs::Link;

use rusqlite::Result;
use serenity::{
    model::channel::Message,
    model::id::{ChannelId, GuildId, MessageId},
    prelude::*,
};
use url::Url;

// largely sourced from newhouse/url-tracking-stripper
const TWITTER_FIELDS: [&str; 1] = ["s"];
const GENERIC_FIELDS: [&str; 28] = [
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
];

#[inline(always)]
fn filter_field(host: &str, field: &str) -> bool {
    let host_match = match host {
        "twitter" | "twitter.com" => TWITTER_FIELDS.contains(&field),
        _ => false,
    };
    host_match || GENERIC_FIELDS.contains(&field)
}

fn filtered_url(url_str: &str) -> Result<Url> {
    let mut url = match Url::parse(url_str) {
        Ok(url) => Ok(url),
        Err(_) => Err(rusqlite::Error::QueryReturnedNoRows),
    }?;

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

fn query_link_matches(url_str: String, server: u64) -> Result<Vec<Link>> {
    let url = filtered_url(&url_str)?;
    let host = url.host_str().ok_or(rusqlite::Error::QueryReturnedNoRows)?;
    let path = url.path();

    let fields = url
        .query_pairs()
        .map(|(field, _)| field.to_string())
        .collect();

    let mut links = Vec::new();
    for link in DB::db_call(|db| db.query_links_host_path_fields(host, path, server, &fields))? {
        if url == filtered_url(&link.link)? {
            links.push(link)
        }
    }
    Ok(links)
}

fn get_link_str(link: &Link) -> String {
    MessageId(link.message).link(ChannelId(link.channel), Some(GuildId(link.server)))
}

impl Handler {
    fn get_links(&self, msg: &str) -> Vec<String> {
        self.finder
            .links(msg)
            .map(|x| x.as_str().to_string())
            .collect()
    }

    pub async fn check_links(&self, ctx: &Context, msg: &Message) {
        let mut reposts = Vec::new();
        let server_id = *msg.guild_id.unwrap().as_u64();
        for link in self.get_links(&msg.content) {
            match query_link_matches(link.clone(), server_id) {
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
                DB::db_call(|db| {
                    db.insert_link(Link {
                        link: link.into(),
                        server: server_id,
                        channel: *msg.channel_id.as_u64(),
                        message: *msg.id.as_u64(),
                        ..Default::default()
                    })
                }),
                "Insert link",
            );
        }

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

            match msg.reply(&ctx.http, format!("REPOST {}", repost_str)).await {
                Ok(_) => (),
                Err(why) => println!("Failed to inform of REPOST: {:?}", why),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn basic_link() {
        let handler = Handler::new();
        let link = "https://twitter.com/user/status/idnumber?s=20";
        let links = handler.get_links(link);

        assert_eq!(links.len(), 1);
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
}
