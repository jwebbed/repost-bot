use super::Message;

#[derive(Debug)]
pub struct Channel {
    pub id: u64,
    pub name: Option<String>,
    pub visible: bool,
    pub server: u64,
}

#[derive(Debug)]
pub struct Link {
    pub id: Option<usize>, // internal to us
    pub link: String,

    pub message: Message,

    // not actually stored in table, just here for convience
    pub channel_name: Option<String>,
    pub server_name: Option<String>,
}
