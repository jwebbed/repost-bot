#![warn(
    clippy::cognitive_complexity,
    clippy::missing_const_for_fn,
    clippy::option_if_let_else
)]

mod errors;
mod handler;
mod structs;

use db;
use log::LevelFilter;
use log::{error, info, warn};
use serenity::model::gateway::GatewayIntents;
use serenity::prelude::*;
use simple_logger::SimpleLogger;
use time::UtcOffset;

use std::env;
use std::process;

use db::DB;
use handler::Handler;

fn migrate_db() {
    match DB::migrate() {
        Ok(_) => info!("sucessfully loaded and migrated db"),
        Err(why) => {
            error!("Failed to migrate, exiting {why:?}");
            process::exit(-1);
        }
    };
}

#[tokio::main]
async fn main() {
    SimpleLogger::new()
        .with_level(LevelFilter::Warn)
        .with_module_level("bot", LevelFilter::Debug)
        .with_module_level("db", LevelFilter::Debug)
        // EST offset, will be incorrect if it runs over DST
        // Could we please abolish DST
        .with_utc_offset(UtcOffset::from_hms(-4, 0, 0).unwrap())
        .init()
        .unwrap();
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    // migrate the db
    migrate_db();

    // init for tokio metrics
    console_subscriber::init();

    let intents = GatewayIntents::GUILDS
        .union(GatewayIntents::GUILD_MEMBERS)
        .union(GatewayIntents::GUILD_MESSAGES)
        .union(GatewayIntents::DIRECT_MESSAGES)
        .union(GatewayIntents::MESSAGE_CONTENT);

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler::new())
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
