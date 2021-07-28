use super::Handler;

use serenity::{model::channel::Message, prelude::*};

impl Handler {
    pub fn handle_command(&self, ctx: &Context, msg: &Message) {
        if !msg.content.starts_with("!rpm") || !msg.content.len() <= 4 {
            return;
        }

        let command = &msg.content[4..].trim();
        match command {
            &"pins" => self.pins(ctx, msg),
            _ => println!("Received unknown command: \"{}\"", command),
        }
    }

    fn pins(&self, ctx: &Context, msg: &Message) {
        println!("Received pins!")
    }
}
