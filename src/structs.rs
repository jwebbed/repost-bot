#[derive(Debug, Default)]
pub struct Link {
    pub id: Option<usize>, // internal to us
    pub link: String,

    // snowflakes referencing the various attributes
    pub server: u64, // called guilds for some reason in discord API
    pub channel: u64,
    pub message: u64,
    // not actually stored in table, just here for convience
    pub channel_name: Option<String>,
    pub server_name: Option<String>,
}

#[derive(Debug, Default)]
pub struct RepostCount {
    pub link: String,
    pub count: u64,
}
