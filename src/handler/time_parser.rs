// Copyright (c) 2016 The humantime Developers
//
// Includes parts of http date with the following copyright:
// Copyright (c) 2016 Pyfisch
//
// Includes portions of musl libc with the following copyright:
// Copyright © 2005-2013 Rich Felker

// below code taken from tailhook/humantime with some of my own modifications
// licensed under MIT

use std::fmt;
use std::time::Duration;

#[derive(Debug, Clone)]
struct FormattedDuration(Duration);

pub fn format_duration(val: Duration) -> String {
    FormattedDuration(val).to_string()
}

impl fmt::Display for FormattedDuration {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let secs = self.0.as_secs();

        if secs == 0 {
            f.write_str("0s")?;
            return Ok(());
        }

        let years = secs / 31_557_600; // 365.25d
        let ydays = secs % 31_557_600;
        let months = ydays / 2_630_016; // 30.44d
        let mdays = ydays % 2_630_016;
        let days = mdays / 86400;
        let day_secs = mdays % 86400;
        let hours = day_secs / 3600;
        let minutes = day_secs % 3600 / 60;
        let seconds = day_secs % 60;

        let values = [
            (years, "year", true),
            (months, "month", true),
            (days, "d", true),
            (hours, "h", false),
            (minutes, "m", false),
            (seconds, "s", false),
        ];

        let mut count = 0;
        for (value, name, is_plural) in values {
            if value > 0 {
                if count > 0 {
                    f.write_str(" ")?;
                }
                write!(f, "{}{}", value, name)?;
                if is_plural && value > 1 {
                    f.write_str("s")?;
                }
                count += 1;
            }

            // Don't include more than 3 terms
            if count >= 3 {
                break;
            }
        }

        Ok(())
    }
}
