use crate::errors::Result;
use crate::handler::bot_read_channel_permission;
use crate::structs::reply::{Reply, ReplyType};

use futures_util::future::try_join_all;
use log::trace;
use serenity::model::id::GuildId;
use serenity::{model::channel::ChannelType, model::channel::Message, prelude::*};
use std::collections::HashMap;
use std::fmt::Write;

pub async fn pins(ctx: &Context, msg: &Message, guild: GuildId) -> Result<Reply> {
    let channels = guild.channels(&ctx.http).await?;
    let pinned_per_channel = try_join_all(
        channels
            .values()
            .filter(|channel| {
                channel.kind == ChannelType::Text && bot_read_channel_permission(ctx, channel)
            })
            .map(|channel| channel.pins(&ctx.http)),
    )
    .await?;

    let ranking = rank_pin_authors(
        pinned_per_channel
            .iter()
            .flatten()
            .map(|pin| pin.author.name.as_str()),
    );
    trace!("found the following pins {ranking:?}");

    Ok(Reply::new(
        format_ranking(&ranking),
        ReplyType::Channel(msg.channel_id),
    ))
}

/// Counts pins per author, sorted by most pins then by name
fn rank_pin_authors<'a>(authors: impl Iterator<Item = &'a str>) -> Vec<(&'a str, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for author in authors {
        *counts.entry(author).or_default() += 1;
    }
    let mut ranking: Vec<_> = counts.into_iter().collect();
    ranking.sort_unstable_by(|(a_name, a_cnt), (b_name, b_cnt)| {
        b_cnt.cmp(a_cnt).then_with(|| a_name.cmp(b_name))
    });
    ranking
}

fn format_ranking(ranking: &[(&str, usize)]) -> String {
    ranking.iter().fold(
        String::from("the chamPIoNship"),
        |mut response, (user, cnt)| {
            let _ = write!(response, "\n{user}: with {cnt} pins");
            response
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rank_pin_authors() {
        let ranking = rank_pin_authors(["b", "a", "c", "a", "c", "a"].into_iter());
        assert_eq!(ranking, vec![("a", 3), ("c", 2), ("b", 1)]);
    }

    #[test]
    fn test_rank_pin_authors_ties_sorted_by_name() {
        let ranking = rank_pin_authors(["z", "y", "x"].into_iter());
        assert_eq!(ranking, vec![("x", 1), ("y", 1), ("z", 1)]);
    }

    #[test]
    fn test_format_ranking() {
        assert_eq!(format_ranking(&[]), "the chamPIoNship");
        assert_eq!(
            format_ranking(&[("a", 3), ("b", 1)]),
            "the chamPIoNship\na: with 3 pins\nb: with 1 pins"
        );
    }
}
