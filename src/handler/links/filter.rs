use crate::errors::Result;

use log::debug;
use phf::phf_set;
use url::Url;

// largely sourced from newhouse/url-tracking-stripper on github

static TWITTER_FIELDS: phf::Set<&'static str> = phf_set! {
    "s",
    "t"
};

static YOUTUBE_FIELDS: phf::Set<&'static str> = phf_set! {
    "feature",
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
        "twitter" | "twitter.com" => TWITTER_FIELDS.contains(field),
        "youtube" | "youtube.com" => YOUTUBE_FIELDS.contains(field),
        _ => false,
    };
    host_match || GENERIC_FIELDS.contains(field)
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
pub fn filtered_url(url_str: &str) -> Result<Url> {
    let base_url = Url::parse(url_str)?;
    debug!("Pre-filter URL: {base_url:?}");
    let mut url = transform_url(base_url)?;
    let host = url.host_str().ok_or(rusqlite::Error::QueryReturnedNoRows)?;

    let fields = url
        .query_pairs()
        .filter(|(field, _value)| !filter_field(host, field))
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

    debug!("Filtered URL: {url:?}");
    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_filter_link() -> Result<()> {
        assert!(!filter_field("www.youtube.com", "v"));
        assert!(filter_field("twitter.com", "s"));

        let filtered = filtered_url("https://twitter.com/user/status/idnumber?s=21")?;
        assert_eq!(
            filtered.as_str(),
            "https://twitter.com/user/status/idnumber"
        );

        Ok(())
    }

    #[test]
    fn test_filter_youtube() -> Result<()> {
        let filtered = filtered_url("https://youtube.com/shorts/fakeid?feature=share")?;
        assert_eq!(filtered.as_str(), "https://youtube.com/shorts/fakeid");
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
