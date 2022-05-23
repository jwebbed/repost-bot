use crate::errors::Result;
use crate::structs::RepostSet;

use async_trait::async_trait;
use serenity::model::prelude::Message;
use std::marker::Sized;

#[async_trait]
pub trait Processor: Sized {
    fn from_message(msg: &Message) -> Result<Self>;

    async fn process(&self) -> Result<()>;

    fn get_repost_set(&self) -> Result<RepostSet>;
}
