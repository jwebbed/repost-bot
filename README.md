# repost-bot

A discord bot to scan, check, and inform users when they're reposting

# To Do

- Identify basic reposts ✅
- Allow direct messaging such that it can inform you if a link is reposted in any mutual servers
- Support images that may be from different links but are otherwise the same

# FAQ

## Why write a discord bot in Rust?

Performance is obviously 🔑 in a discord bot

## If performance is key why aren't you using a real database instead of SQLite

Arguably for this purpose the perfomance of any other database will actually be worse because of the downsides of IPC. You also might have missed the sarcasm in the previous response

## Why is your SQL so bad

Most of my time with SQL code is writing read operations not write operations, so I'm understandably rusty. Please feel free to fork and improve.
