# Loki
The trickster ~~god~~ bot, for all your friend group's memetic
Discord server needs.

## Limitations

This is a personal project intended specifically for one of my
friend groups' servers; whilst it's intended to be implemented
reasonably generically, features themselves are tailored to that
server (alone).

## Planned features

- [x] Command to explain my Discord status.
    - [x] Management command so that the bot owner can update the
          response to this.
- [ ] Nickname auto-changer lottery
- [ ] Responses to specific text in messages (but not actual commands)
- [ ] Periodic checks for how many known issues are present in FH5
and compares to the same list for GT7. Output in number of pages.
- [x] "Meme of the week"
    - Once started, it watches the memes channel.
    - After 5 days, if no memes have been posted, it posts a
      reminder.
    - After a further two days, it tallies up all reactions to the
      posts. The post with the greatest number of reactions wins,
      and the system resets for the next week.
- [ ] Reminders.
  - Probably generic reminders, set by server admins.
- [x] Automatic nickname updates when live on Twitch.
  - I will never understand why the built-in "Streamer mode" on
    Discord simply doesn't do this. Having to check the status
    to see if someone in a VC is streaming is, frankly, silly.
- [x] Event system
  - Users may choose to receive specific bot events, which will be
    DM'd to them when the event fires. This feature is a prelude of
    the Reminders feature.
- [ ] Monitoring system for timeouts
  - Track aggregate data about how many times a user has been timed
    out, and the total time they have been timed out for.

Eventually roll [ThreadReviver](https://github.com/Lyrenhex/ThreadReviver)'s behaviour into Loki.

## Getting started

### Gateway Intents

The bot requires the following privileged Gateway Intents:
- `GUILD_PRESENCES`

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
