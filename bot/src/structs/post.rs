use crate::errors::Result;
use bytes::Bytes;
use log::info;
use serenity::model::channel;
use serenity::model::event::MessageUpdateEvent;
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

/// Shared so connections (and TLS sessions) are pooled across downloads
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("failed to build http client")
});

#[derive(Debug, PartialEq, Eq)]
pub enum AttachmentType {
    Attachment {
        content_type: Option<String>,
    },
    EmbedImage {
        provider_name: Option<Box<str>>,
    },
    EmbedThumbnail {
        provider_name: Option<Box<str>>,
        square_dimension: Option<u32>,
        is_link_type: bool,
    },
}

#[derive(Debug)]
pub struct Attachment {
    pub url: Arc<str>,
    pub attachment_type: AttachmentType,
    /// For embeds, the url of the page the embed is a preview of
    pub source_url: Option<Box<str>>,
}

impl Attachment {
    #[inline]
    pub fn from_attachment(attachment: &channel::Attachment) -> Attachment {
        Attachment {
            url: attachment.url.as_str().into(),
            attachment_type: AttachmentType::Attachment {
                content_type: attachment.content_type.clone(),
            },
            source_url: None,
        }
    }

    #[inline]
    pub fn from_embed_image(embed: &channel::Embed, image: &channel::EmbedImage) -> Attachment {
        Self::from_embed(
            AttachmentType::EmbedImage {
                provider_name: get_provider_name(embed.provider.as_ref()),
            },
            embed,
            image.proxy_url.as_deref(),
            &image.url,
        )
    }

    #[inline]
    pub fn from_embed_thumbnail(
        embed: &channel::Embed,
        image: &channel::EmbedThumbnail,
    ) -> Attachment {
        Self::from_embed(
            AttachmentType::EmbedThumbnail {
                provider_name: get_provider_name(embed.provider.as_ref()),
                square_dimension: get_square_embed_dimension(image),
                is_link_type: embed.kind.as_deref() == Some("link"),
            },
            embed,
            image.proxy_url.as_deref(),
            &image.url,
        )
    }

    #[inline]
    fn from_embed(
        attachment_type: AttachmentType,
        embed: &channel::Embed,
        proxy_url: Option<&str>,
        url: &str,
    ) -> Attachment {
        Attachment {
            url: proxy_url.unwrap_or(url).into(),
            attachment_type,
            source_url: embed.url.as_deref().map(Box::from),
        }
    }

    pub async fn download(&self) -> Result<Bytes> {
        let download_time = Instant::now();
        let data = HTTP_CLIENT
            .get(&*self.url)
            .send()
            .await?
            .error_for_status()?
            .bytes()
            .await?;
        info!(
            "downloaded {} bytes in {:.2?} from {}",
            data.len(),
            download_time.elapsed(),
            self.url
        );
        Ok(data)
    }
}

#[derive(Debug)]
pub struct Post {
    pub db_message: db::structs::Message,
    content: String,
    attachments: Vec<Attachment>,
}

impl Post {
    #[inline]
    pub fn from_message(db_message: &db::structs::Message, message: &channel::Message) -> Post {
        Post::new(
            db_message,
            &message.content,
            &message.attachments,
            &message.embeds,
        )
    }

    #[inline]
    pub fn from_update(db_message: &db::structs::Message, event: &MessageUpdateEvent) -> Post {
        Post::new(
            db_message,
            event.content.as_deref().unwrap_or_default(),
            event.attachments.as_deref().unwrap_or_default(),
            event.embeds.as_deref().unwrap_or_default(),
        )
    }

    fn new(
        message: &db::structs::Message,
        content: &str,
        msg_attachments: &[channel::Attachment],
        msg_embeds: &[channel::Embed],
    ) -> Post {
        let embed_attachments = msg_embeds.iter().flat_map(|embed| {
            let image = embed
                .image
                .as_ref()
                .map(|image| Attachment::from_embed_image(embed, image));
            let thumbnail = embed
                .thumbnail
                .as_ref()
                .map(|thumbnail| Attachment::from_embed_thumbnail(embed, thumbnail));
            image.into_iter().chain(thumbnail)
        });
        let attachments = msg_attachments
            .iter()
            .map(Attachment::from_attachment)
            .chain(embed_attachments)
            .collect();

        Post {
            db_message: *message,
            content: content.into(),
            attachments,
        }
    }

    #[inline]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[inline]
    pub const fn server_id(&self) -> u64 {
        self.db_message.server
    }

    #[inline]
    pub const fn id(&self) -> u64 {
        self.db_message.id
    }

    #[inline]
    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    #[inline]
    pub fn attachments(&self) -> impl Iterator<Item = &Attachment> {
        self.attachments.iter()
    }
}

#[inline]
fn get_square_embed_dimension(embed: &channel::EmbedThumbnail) -> Option<u32> {
    // This if will pass even when both width and height are none, however
    // if we added a check to ensure the option is some, the alternative is
    // we'd just return None anyways so this works out to be the same result.
    if embed.width == embed.height {
        embed.width
    } else {
        None
    }
}

#[inline]
fn get_provider_name(provider_option: Option<&channel::EmbedProvider>) -> Option<Box<str>> {
    provider_option
        .and_then(|provider| provider.name.as_deref())
        .map(Box::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;

    fn db_message() -> db::structs::Message {
        db::structs::Message::new(1, 2, 3, None, Utc::now(), None, None, None, None)
    }

    fn attachment(url: &str, content_type: Option<&str>) -> channel::Attachment {
        serde_json::from_value(json!({
            "id": "1",
            "filename": "file",
            "size": 1,
            "url": url,
            "proxy_url": format!("{url}?proxy"),
            "content_type": content_type,
        }))
        .unwrap()
    }

    fn embed(value: serde_json::Value) -> channel::Embed {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn test_post_from_attachments_and_embeds() {
        let attachments = [attachment("https://cdn/a.png", Some("image/png"))];
        let embeds = [
            embed(json!({
                "type": "link",
                "url": "https://threads.net/post",
                "provider": { "name": "Threads" },
                "thumbnail": { "url": "https://t/1.png", "proxy_url": "https://proxy/1.png", "width": 100, "height": 100 },
            })),
            embed(json!({
                "type": "rich",
                "image": { "url": "https://i/2.png" },
                "thumbnail": { "url": "https://t/3.png", "width": 100, "height": 50 },
            })),
            embed(json!({ "type": "rich", "title": "no images" })),
        ];
        let post = Post::new(&db_message(), "content", &attachments, &embeds);

        assert_eq!(post.content(), "content");
        assert_eq!(post.id(), 1);
        assert_eq!(post.server_id(), 2);
        assert!(post.has_attachments());
        let source_urls: Vec<_> = post
            .attachments()
            .map(|a| a.source_url.as_deref())
            .collect();
        assert_eq!(
            source_urls,
            vec![None, Some("https://threads.net/post"), None, None]
        );

        let found: Vec<_> = post
            .attachments()
            .map(|a| (&*a.url, &a.attachment_type))
            .collect();
        assert_eq!(
            found,
            vec![
                (
                    "https://cdn/a.png",
                    &AttachmentType::Attachment {
                        content_type: Some("image/png".into())
                    }
                ),
                // proxy url is preferred when available
                (
                    "https://proxy/1.png",
                    &AttachmentType::EmbedThumbnail {
                        provider_name: Some("Threads".into()),
                        square_dimension: Some(100),
                        is_link_type: true,
                    }
                ),
                (
                    "https://i/2.png",
                    &AttachmentType::EmbedImage {
                        provider_name: None
                    }
                ),
                (
                    "https://t/3.png",
                    &AttachmentType::EmbedThumbnail {
                        provider_name: None,
                        square_dimension: None,
                        is_link_type: false,
                    }
                ),
            ]
        );
    }

    #[test]
    fn test_post_without_attachments() {
        let post = Post::new(&db_message(), "just text", &[], &[]);
        assert!(!post.has_attachments());
        assert_eq!(post.attachments().count(), 0);
    }

    #[test]
    fn test_post_from_update_defaults() {
        let event: MessageUpdateEvent = serde_json::from_value(json!({
            "id": "1",
            "channel_id": "3",
        }))
        .unwrap();
        let post = Post::from_update(&db_message(), &event);
        assert_eq!(post.content(), "");
        assert!(!post.has_attachments());
    }
}
