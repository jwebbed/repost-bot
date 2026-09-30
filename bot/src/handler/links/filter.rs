use crate::errors::{Error, Result};

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
#[inline]
fn filter_field(host: &str, field: &str) -> bool {
    let host_match = match host {
        "twitter" | "twitter.com" | "x" | "x.com" => TWITTER_FIELDS.contains(field),
        "youtube" | "youtube.com" | "www.youtube.com" | "m.youtube.com" => {
            YOUTUBE_FIELDS.contains(field)
        }
        _ => false,
    };
    host_match || GENERIC_FIELDS.contains(field)
}

fn transform_url(url: Url) -> Result<Url> {
    let transformed = match url.host_str() {
        Some("youtu.be") => match url.path().strip_prefix('/') {
            Some(id) if !id.is_empty() => Some(Url::parse_with_params(
                "https://www.youtube.com/watch",
                &[("v", id)],
            )?),
            _ => None,
        },
        Some("x.com") => {
            let mut new_url = url.clone();
            new_url.set_host(Some("twitter.com"))?;
            new_url.set_path(&url.path().to_ascii_lowercase());
            Some(new_url)
        }
        Some("twitter.com") => {
            let mut new_url = url.clone();
            new_url.set_path(&url.path().to_ascii_lowercase());
            Some(new_url)
        }
        _ => None,
    };

    Ok(transformed.unwrap_or(url))
}

/// Hosts of music streaming services. They all use the album cover as the
/// preview image for songs, so they are treated as a single site.
static MUSIC_STREAMING_HOSTS: phf::Set<&'static str> = phf_set! {
    "open.spotify.com",
    "play.spotify.com",
    "spotify.link",
    "music.apple.com",
    "geo.music.apple.com",
    "itunes.apple.com",
    "tidal.com",
    "listen.tidal.com",
    "deezer.com",
    "link.deezer.com",
    "deezer.page.link",
    "music.youtube.com",
    "soundcloud.com",
    "m.soundcloud.com",
    "on.soundcloud.com",
    "pandora.com",
    "song.link",
    "album.link",
    "odesli.co",
};

/// Site key shared by every music streaming service
const MUSIC_STREAMING_SITE: &str = "music streaming";

fn is_music_streaming_host(host: &str) -> bool {
    MUSIC_STREAMING_HOSTS.contains(host)
        // every artist has their own subdomain
        || host == "bandcamp.com"
        || host.ends_with(".bandcamp.com")
        // one domain per country, e.g. music.amazon.co.uk
        || host.starts_with("music.amazon.")
}

/// Returns the site a url belongs to, ignoring any leading "www.". Every
/// music streaming service is considered to be the same site.
pub fn site_host(url: &Url) -> Option<&str> {
    url.host_str().map(|host| {
        let host = host.strip_prefix("www.").unwrap_or(host);
        if is_music_streaming_host(host) {
            MUSIC_STREAMING_SITE
        } else {
            host
        }
    })
}

/// filtered_url takes a url_str and returns a Url object with the any irrelevent
/// fields in the query string removed as per filter_field
pub fn filtered_url(url_str: &str) -> Result<Url> {
    let base_url = Url::parse(url_str)?;
    debug!("Pre-filter URL: {base_url:?}");
    let mut url = transform_url(base_url)?;
    let host = url.host_str().ok_or(Error::ConstStr("URL has no host"))?;

    // only rebuild the query string when there is one to filter
    if url.query().is_some() {
        let fields: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(field, _value)| !filter_field(host, field))
            .map(|(f, v)| (f.into_owned(), v.into_owned()))
            .collect();

        if fields.is_empty() {
            url.set_query(None);
        } else {
            url.query_pairs_mut().clear().extend_pairs(fields);
        }
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
    fn test_twitter_case_insenstive() -> Result<()> {
        let url_lower = filtered_url("https://twitter.com/name/status/000")?;
        let url_cased = filtered_url("https://twitter.com/NaMe/status/000")?;
        assert_eq!(url_lower, url_cased);
        Ok(())
    }

    #[test]
    fn test_x_transformed() -> Result<()> {
        let url = Url::parse("https://x.com/fake_user/status/12345?s=46")?;
        assert_eq!(
            transform_url(url)?.as_str(),
            "https://twitter.com/fake_user/status/12345?s=46"
        );
        Ok(())
    }

    #[test]
    fn text_filter_x() -> Result<()> {
        assert_eq!(
            filtered_url("https://x.com/fake_user/status/12345?s=46")?.as_str(),
            "https://twitter.com/fake_user/status/12345"
        );
        Ok(())
    }

    #[test]
    fn test_x_case_insenstive() -> Result<()> {
        let url_lower = filtered_url("https://x.com/name/status/000")?;
        let url_cased = filtered_url("https://x.com/NaMe/status/000")?;
        assert_eq!(url_lower, url_cased);
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

    #[test]
    fn test_youtube_sl_with_params() -> Result<()> {
        let url = Url::parse("https://youtu.be/anotherfakeid?si=fakeparam")?;
        assert_eq!(
            transform_url(url)?.as_str(),
            "https://www.youtube.com/watch?v=anotherfakeid"
        );
        Ok(())
    }

    #[test]
    fn test_youtube_sl_without_id() -> Result<()> {
        let url = Url::parse("https://youtu.be/")?;
        assert_eq!(transform_url(url)?.as_str(), "https://youtu.be/");
        Ok(())
    }

    #[test]
    fn test_filter_www_youtube() -> Result<()> {
        let filtered = filtered_url("https://www.youtube.com/watch?v=fakeid&feature=share")?;
        assert_eq!(filtered.as_str(), "https://www.youtube.com/watch?v=fakeid");
        Ok(())
    }

    #[test]
    fn test_filter_generic_tracking_fields() -> Result<()> {
        let filtered = filtered_url(
            "https://example.com/article?utm_source=a&id=5&fbclid=b&utm_campaign=c&page=2",
        )?;
        assert_eq!(filtered.as_str(), "https://example.com/article?id=5&page=2");
        Ok(())
    }

    #[test]
    fn test_filter_keeps_fields_for_other_hosts() -> Result<()> {
        // "s" and "t" are only tracking fields on specific hosts
        let filtered = filtered_url("https://example.com/search?s=term&t=10")?;
        assert_eq!(filtered.as_str(), "https://example.com/search?s=term&t=10");
        Ok(())
    }

    #[test]
    fn test_filter_all_fields_removes_question_mark() -> Result<()> {
        assert_eq!(
            filtered_url("https://example.com/?utm_source=a")?.as_str(),
            "https://example.com/"
        );
        assert_eq!(
            filtered_url("https://example.com/?")?.as_str(),
            "https://example.com/"
        );
        Ok(())
    }

    #[test]
    fn test_filter_preserves_fragment() -> Result<()> {
        assert_eq!(
            filtered_url("https://example.com/page?ref=a#section")?.as_str(),
            "https://example.com/page#section"
        );
        Ok(())
    }

    #[test]
    fn test_filter_url_errors() {
        assert!(matches!(filtered_url("not a url"), Err(Error::Url(_))));
        assert!(matches!(
            filtered_url("mailto:someone@example.com"),
            Err(Error::ConstStr(_))
        ));
    }

    #[test]
    fn test_site_host() -> Result<()> {
        let site = |url: &str| Url::parse(url).map(|url| site_host(&url).map(String::from));
        assert_eq!(
            site("https://www.example.com/a")?.as_deref(),
            Some("example.com")
        );
        assert_eq!(
            site("https://example.com/b")?.as_deref(),
            Some("example.com")
        );
        assert_eq!(site("mailto:someone@example.com")?, None);
        Ok(())
    }

    #[test]
    fn test_music_streaming_sites_are_one_site() -> Result<()> {
        let site = |url: &str| Url::parse(url).map(|url| site_host(&url).map(String::from));
        for url in [
            "https://open.spotify.com/track/abc",
            "https://music.apple.com/ca/album/x/1?i=2",
            "https://listen.tidal.com/track/1",
            "https://tidal.com/browse/track/1",
            "https://www.deezer.com/track/1",
            "https://music.youtube.com/watch?v=abc",
            "https://soundcloud.com/artist/song",
            "https://artist.bandcamp.com/track/song",
            "https://music.amazon.co.uk/albums/abc",
            "https://song.link/s/abc",
        ] {
            assert_eq!(
                site(url)?.as_deref(),
                Some(MUSIC_STREAMING_SITE),
                "{url} should be a music streaming site"
            );
        }
        // regular youtube videos and lookalike hosts aren't music streaming
        for url in [
            "https://www.youtube.com/watch?v=abc",
            "https://notbandcamp.com/",
            "https://apple.com/music",
        ] {
            assert_ne!(site(url)?.as_deref(), Some(MUSIC_STREAMING_SITE), "{url}");
        }
        Ok(())
    }
}
