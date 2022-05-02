//! A utils module.
//!
//! A utils file is an anti-pattern that I regretably just created. I make
//! an exception in this circumstance as the goal is to remove this file
//! and function entirely when I am able to get a better solution to this
//! problem.

use crate::errors::{Error, Result};
use chrono::{DateTime, FixedOffset};
use log::trace;
use serenity::model::Timestamp;

/// I actually hate this function but serenity made a change to use their own
/// internal Timestamp type, which means I need to do some weird logic to
/// get a regular Datetime struct. I should probably optimize this to not
/// do some really silly string parsing but for now it works.
#[inline(always)]
pub fn convert_serenity_datetime(serenity_dt: Timestamp) -> Result<DateTime<FixedOffset>> {
    let datetime_str = &serenity_dt.to_rfc3339();
    trace!("rfc333 datetime str is {datetime_str}");

    DateTime::parse_from_rfc3339(&datetime_str)
        .map_err(|err| Error::Internal(format!("Datetime couldn't be converted with err {err:?}")))
}
