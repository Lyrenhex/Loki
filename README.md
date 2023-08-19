# Loki
The trickster ~~god~~ bot, for all your friend group's memetic
Discord server needs.

## Limitations

This is a personal project intended specifically for one of my
friend groups' servers; whilst it's intended to be implemented
reasonably generically, features themselves are tailored to that
server (alone).

## Planned features

Each implemented feature is enabled by default.
You may enable specific feature sets by using `--no-default-features` at compile-time,
and supply only the feature flags which you wish to enable (please see the section on
Privileged Intents below).

- [x] Command to explain my Discord status. (`status-meaning`)
    - [x] Management command so that the bot owner can update the
          response to this.
- [x] Nickname auto-changer lottery. (`nickname-lottery`)
- [x] Responses to specific text in messages (but not actual commands) (`text-response`)
- [ ] Periodic checks for how many known issues are present in FH5
and compares to the same list for GT7. Output in number of pages.
- [x] "Meme of the week" (`memes`)
    - Once started, it watches the memes channel.
    - After 5 days, if no memes have been posted, it posts a
      reminder.
    - After a further two days, it tallies up all reactions to the
      posts. The post with the greatest number of reactions wins,
      and the system resets for the next week.
- [ ] Reminders.
  - Probably generic reminders, set by server admins.
- [x] Automatic nickname updates when live on Twitch. (`stream-indicator`)
  - I will never understand why the built-in "Streamer mode" on
    Discord simply doesn't do this. Having to check the status
    to see if someone in a VC is streaming is, frankly, silly.
- [x] Event system (`events`)
  - Users may choose to receive specific bot events, which will be
    DM'd to them when the event fires. This feature is a prelude of
    the Reminders feature.
- [x] Monitoring system for timeouts (`timeout-monitor`)
  - Track aggregate data about how many times a user has been timed
    out, and the total time they have been timed out for.
- [x] Revive threads when they get archived. (`thread_reviver`)
  - This requires `MANAGE_THREADS` permission.
  - This is (and supersedes) [ThreadReviver](https://github.com/Lyrenhex/ThreadReviver).

### Gateway Intents

The following lists detail which feature flags require specific privileged intents to function.
Enabling any of these feature flags will automatically enable the required intent's feature flag;
you must ensure that the bot is configured to use these intents in the Discord Developer Portal.

> All features are enabled (and thus all intents required) by default.

- **Guild Presences** (`guild-presences`)
  - `stream-indicator`

- **Server Members** (`guild-members`)
  - `timeout-monitor`

- **Message Content** (`message-content`)
  - `text-response`

## Getting started

### Configuration

Configuration takes place in `config.toml`, which by default should be in the same place as where the
bot is running; however, this can be changed by specifying the path in the `LOKI_CONFIG_PATH` environment
variable.

A minimal `config.toml` looks like this (note that the bot will write to this file, so any comments will
likely be lost):

```toml
manager = "123456789012345678" # your Discord User ID.

[tokens]
discord = "some alphanumeric characters" # your Discord bot's token, from the Discord developer dashboard.
```

IDs, such as your User ID, should be obtained by using the "Copy ID" functionality in Discord
Developer mode.

**WARNING:** This bot is a personal project, _and is currently pre-release per SemVer_; the configuration
structure **is subject to change**, and configuration structures **will not** migrate cleanly over between
versions. You may lose data, and may find it easier to nuke your data anyway when the config structure
changes. Again, **this is not production-ready software** and you have been warned: there's no warranty!

### Running the bot

`cargo run --release`

**Note:** In debug mode, the bot is desiged to use a testing server.
To compile the bot in debug mode, you will need to set the `LOKI_DEBUG_GUILD_ID`
environment variable to the Guild ID of _your_ server.
This can be found in the Discord application by enabling Developer mode, right
clicking your server and clicking "Copy ID". Store that in the environment variable,
reload your shell, et voíla - you're done!

> For the curious, this is because in debug mode we switch to guild-specific
> commands, which update instantly. In release mode, commands are global, which
> bears an up-to-1-hour propagation delay when command structures are updated.

## Credits

This is a personal project. That said, there are code snippets either
heavily inspired by (or modified versions of) code from
[parrot](https://github.com/aquelemiguel/parrot).

Many thanks to their contributors!
