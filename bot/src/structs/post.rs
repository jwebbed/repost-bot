use crate::errors::Result;
use async_once_cell::Lazy;
use log::info;
use serenity::model;
use serenity::model::channel;
use serenity::model::event::MessageUpdateEvent;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

#[derive(Debug)]
pub enum AttachmentType {
    Attachment {
        content_type: Option<String>,
    },
    EmbedImage {
        provider: Option<channel::EmbedProvider>,
    },
    EmbedThumbnail {
        provider: Option<channel::EmbedProvider>,
        square_dimension: Option<u32>,
        is_link_type: bool,
    },
}

type LazyFuture<T> = Lazy<T, Pin<Box<dyn Future<Output = T> + std::marker::Send>>>;

#[derive(Debug)]
pub struct Attachment {
    pub url: Arc<str>,
    pub attachment_type: AttachmentType,
    attachment_bytes: LazyFuture<Result<Vec<u8>>>,
}

impl Attachment {
    #[inline]
    pub fn from_attachment(attachment: &channel::Attachment) -> Attachment {
        let url: Arc<str> = attachment.url.clone().into();
        let download = download(url.clone());
        Attachment {
            url,
            attachment_type: AttachmentType::Attachment {
                content_type: attachment.content_type.clone(),
            },
            attachment_bytes: Lazy::new(Box::pin(download)),
        }
    }

    #[inline]
    pub fn from_embed_image(embed: &channel::Embed, image: &channel::EmbedImage) -> Attachment {
        Self::from_embed(
            AttachmentType::EmbedImage {
                provider: embed.provider.clone(),
            },
            &image.proxy_url,
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
                provider: embed.provider.clone(),
                square_dimension: get_square_embed_dimension(image),
                is_link_type: embed.kind.as_ref().map_or(false, |kind| kind == "link"),
            },
            &image.proxy_url,
            &image.url,
        )
    }

    #[inline]
    fn from_embed(
        attachment_type: AttachmentType,
        proxy_url: &Option<String>,
        url: &str,
    ) -> Attachment {
        let url: Arc<str> = proxy_url
            .as_ref()
            .map_or_else(|| url.into(), |proxy_url| proxy_url.clone().into());
        let download = download(url.clone());
        Attachment {
            url,
            attachment_type,
            attachment_bytes: Lazy::new(Box::pin(download)),
        }
    }

    #[inline(always)]
    pub async fn download(&self) -> Result<&Vec<u8>> {
        let download_time = Instant::now();
        let data = self.attachment_bytes.get_unpin().await.as_ref()?;
        info!(
            "downloaded {} bytes in {:.2?} from {}",
            data.len(),
            download_time.elapsed(),
            self.url
        );
        Ok(data)
    }
}

#[derive(Debug, Clone)]
pub struct Post {
    pub db_message: db::structs::Message,
    content: Arc<str>,
    attachments: Arc<[Attachment]>,
}

impl Post {
    #[inline]
    pub fn from_message(
        db_message: &db::structs::Message,
        message: &model::channel::Message,
    ) -> Post {
        Post::new(
            db_message,
            &message.content,
            &message.attachments,
            &message.embeds,
        )
    }

    #[inline]
    pub fn from_update(db_message: &db::structs::Message, event: &MessageUpdateEvent) -> Post {
        let content_default = "";
        let attachments_default = vec![];
        let embeds_default = vec![];
        Post::new(
            db_message,
            event
                .content
                .as_ref()
                .map_or(content_default, |content| content),
            event.attachments.as_ref().unwrap_or(&attachments_default),
            event.embeds.as_ref().unwrap_or(&embeds_default),
        )
    }

    fn new(
        message: &db::structs::Message,
        content: &str,
        msg_attachments: &[channel::Attachment],
        msg_embeds: &[channel::Embed],
    ) -> Post {
        let mut attachments = Vec::with_capacity(msg_attachments.len() + msg_embeds.len() * 2);
        for attachment in msg_attachments {
            attachments.push(Attachment::from_attachment(attachment))
        }
        for embed in msg_embeds {
            if let Some(image) = &embed.image {
                attachments.push(Attachment::from_embed_image(embed, image));
            }

            if let Some(thumbnail) = &embed.thumbnail {
                attachments.push(Attachment::from_embed_thumbnail(embed, thumbnail));
            }
        }
        Post {
            db_message: *message,
            content: content.into(),
            attachments: attachments.into(),
        }
    }

    #[inline(always)]
    pub const fn content(&self) -> &Arc<str> {
        &self.content
    }

    #[inline(always)]
    pub const fn server_id(&self) -> u64 {
        self.db_message.server
    }

    #[inline(always)]
    pub const fn id(&self) -> u64 {
        self.db_message.id
    }

    #[inline(always)]
    pub fn has_attachments(&self) -> bool {
        !self.attachments.is_empty()
    }

    #[inline(always)]
    pub fn attachments(&self) -> impl Iterator<Item = &Attachment> {
        // Should convert this to a stream
        self.attachments.iter()
    }
}

#[inline(always)]
async fn download(url: Arc<str>) -> Result<Vec<u8>> {
    Ok(reqwest::get(&*url).await?.bytes().await?.to_vec())
}

#[inline(always)]
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
