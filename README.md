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
  - `/status_meaning` (universal)
  - [x] Management command so that the bot owner can update the
        response to this.
    - `/set_status_meaning` (universal, but the bot restricts its functionality to the `Manager`).
- [x] Nickname auto-changer lottery. (`nickname-lottery`)
  - `/nickname_lottery set_nicknames {user}` (MANAGE_NICKNAMES)
    - Opens a Discord form to set the nicknames, pre-filled with the existing list (if any). Nicknames are separated by a newline, and leading and trailing whitespace is stripped. Nicknames are truncated to 30 characters.
  - Nickname changes occur at a random, changing interval between 30 minutes and 5 days, or every 30 minutes on April Fool's (beginning at midnight).
    - Note: The current interval does not persist across a restart, so it can be up to a maximal 10 days before a nickname is changed.
- [x] Responses to specific text in messages (but not actual commands) (`text-response`)
  - `/response list` (ADMINISTRATOR)
    - List any currently-set phrases and their response.
  - `/response set {activation_phrase}` (ADMINISTRATOR)
    - Set a new response to the given activation phrase.
- [ ] Periodic checks for how many known issues are present in FH5
and compares to the same list for GT7. Output in number of pages.
- [x] "Meme of the week" (`memes`)
  - Once started, it watches the memes channel.
  - After 5 days, if no memes have been posted, it posts a
    reminder.
  - After a further two days, it tallies up all reactions to the
    posts. The post with the greatest number of reactions wins,
    and the system resets for the next week.
  - `/memes set_channel {channel}` (MANAGE_CHANNELS)
    - Set the channel to monitor for memes, and starts the countdown from when this command is issued.
    - This resets the timer and memes list if a channel was already set.
  - `/memes unset_channel`
    - Unsets the channel, thus disabling this functionality until a new channel is set.
- [ ] Reminders.
  - Probably generic reminders, set by server admins.
- [x] Automatic nickname updates when live on Twitch. (`stream-indicator`)
  - Prepends `🔴 ` to the start of a user's nickname when they go live, and removes it when they stop.
  - This does not work for any users who have a role above the bot, or (in any case) the Server Owner.
  - The user must have their Twitch linked to their Discord account, and have broadcasts shared through their Discord Presence.
  - I will never understand why the built-in "Streamer mode" on
    Discord simply doesn't do this. Having to check the status
    to see if someone in a VC is streaming is, frankly, silly.
- [x] Event system (`events`)
  - Users may choose to receive specific bot events, which will be
    DM'd to them when the event fires. This feature is a prelude of
    the Reminders feature.
  - `/events subscribe {event}` (universal)
  - `/events unsubscribe {event}` (universal)
- [x] Monitoring system for timeouts (`timeout-monitor`)
  - Track aggregate data about how many times a user has been timed
    out, and the total time they have been timed out for.
  - Can be queried at will, and the number of timeouts can be announced in a specified channel when a user is timed out.
  - `/timeouts check {user}` (USE_SLASH_COMMANDS)
    - Get the number of times, and total time, a user was timed out.
  - `/timeouts configure_announcements {channel?} {announcement_prefix?}` (MANAGE_CHANNELS)
    - Sets the announcement channel to `channel` if supplied.
    - Sets the announcement prefix (which is prepended to the announcement message), if supplied. Note that this is not required, but provided in case of server-specific emoji which is intended to be included.
    - Attempting to set a prefix whilst having never set the channel will fail; a channel must be set first (or at the same time), but does not need to be supplied with every use of this command.
  - `/timeouts stop_announcements` (MANAGE_CHANNELS)
    - Stops the announcements when a user is timed out, and unsets any prefix.
- [x] Revive threads when they get archived. (`thread_reviver`)
  - This requires `MANAGE_THREADS` permission.
  - This is (and supersedes) [ThreadReviver](https://github.com/Lyrenhex/ThreadReviver).
- [x] Scoreboards (`scoreboard`)
  - `/create_scoreboard {scoreboard name}` (ADMINISTRATOR)
  - `/scoreboard delete {scoreboard name}` (ADMINISTRATOR)
  - `/scoreboard view {scoreboard_name} {user?}`
    - Displays either the top 10 _or_ the score (and place in the leaderboard) of the specified user.
  - `/scoreboard set {scoreboard name} {score}`
    - Sets the calling user's score to the specified `score`.
  - `/scoreboard override {scoreboard name} {user} {score}` (ADMINISTRATOR)
    - Overrides the `user`'s score to the specified `score`.

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
