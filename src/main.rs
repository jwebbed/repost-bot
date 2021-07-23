mod db;
mod handler;
mod structs;

use linkify::LinkFinder;
use serenity::prelude::*;
use std::env;
use std::process;

use db::get_db;

fn migrate_db() {
    match get_db() {
        Ok(db) => match db.migrate() {
            Ok(_) => println!("Sucessfully loaded and migrated db"),
            Err(why) => {
                println!("Failed to migrate, exiting {:?}", why);
                process::exit(-1);
            }
        },
        Err(why) => {
            println!("Failed to get db, exiting {:?}", why);
            process::exit(-1)
        }
    };
}

#[tokio::main]
async fn main() {
    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let handler = handler::Handler {
        finder: LinkFinder::new(),
    };

    // get and migrate the db
    migrate_db();

    // Create a new instance of the Client, logging in as a bot. This will
    // automatically prepend your bot token with "Bot ", which is a requirement
    // by Discord for bot users.
    let mut client = Client::builder(&token)
        .event_handler(handler)
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