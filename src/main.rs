mod db;
mod handler;
mod structs;

use serenity::prelude::*;
use std::env;
use std::process;

use db::DB;
use handler::Handler;

fn migrate_db() {
    match DB::migrate() {
        Ok(_) => println!("Sucessfully loaded and migrated db"),
        Err(why) => {
            println!("Failed to migrate, exiting {:?}", why);
            process::exit(-1);
        }
    };
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    // migrate the db
    migrate_db();

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = Client::builder(&token)
        .event_handler(Handler {})
        .await
        .expect("Err creating client");

    // Finally, start a single shard, and start listening to events.
    //
    // Shards will automatically attempt to reconnect, and will perform
    // exponential backoff until it reconnects.
    if let Err(why) = client.start().await {
        println!("Client error: {:?}", why);
    }
}
