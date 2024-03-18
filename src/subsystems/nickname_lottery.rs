use std::{collections::HashMap, str::FromStr, time::Duration};

use chrono::{Datelike, TimeZone};
use log::{error, info, warn};
use rand::{
    distributions::Distribution,
    seq::{IteratorRandom, SliceRandom},
};
use serde::{Deserialize, Serialize};
use serenity::{
    all::{CacheHttp as _, CommandDataOptionValue, CreateModal, Guild, Mentionable as _, UserId},
    async_trait,
    model::{channel::ChannelType, id::ChannelId, Permissions},
    prelude::Context,
};

#[cfg(feature = "events")]
use crate::{command::notify_subscribers, subsystems::events::Event};

use crate::{
    command::OptionType, config::Config, create_embed, create_raw_embed,
    notify_subscribers_with_handle, ActionResponse,
};
use crate::{
    command::{Command, PermissionType},
    get_guild,
};

use super::Subsystem;

/// (30 mins, 5 days) in seconds.
const REFRESH_INTERVAL: (u64, u64) = (1_800, 432_000);

#[derive(Default)]
pub struct NicknameLottery;

#[derive(Serialize, Deserialize, Default)]
pub struct NicknameLotteryGuildData {
    /// HashMap of stringified [UserId]s to their respective list of specific nicknames, or [None] if they are excluded from the system.
    user_specific_nicknames: HashMap<String, Vec<String>>,
    /// Channel that the bot demands a name change in, if it fails to do so itself. The bot will 'silently' fail if this is not set.
    complaint_channel: Option<ChannelId>,
    /// An override for the title of the bot name change demand. Uses default if [None].
    title_override: Option<String>,
}

impl NicknameLotteryGuildData {
    /// Deconstructs a list of nicknames, separated by newlines, to into a [Vec].
    pub fn deconstruct_nickname_string(nicks: &str) -> Vec<String> {
        nicks
            .split('\n')
            .filter_map(|n| {
                let n = n.trim();
                if n.is_empty() {
                    return None;
                }
                Some(n.to_string().chars().take(30).collect())
            })
            .collect()
    }

    /// Constructs a [String] listing the nicknames separated by newlines.
    pub fn construct_nickname_string(nicks: &[String]) -> String {
        nicks.join("\n")
    }

    /// Returns the list of specific nicknames for a given [UserId] as a [String], with each entry on a separate line.
    pub fn user_nicknames_string(&self, user: &UserId) -> String {
        self.user_specific_nicknames
            .get(&user.to_string())
            .map_or(String::default(), |n| Self::construct_nickname_string(n))
    }

    /// Sets the list of specific nicknames based for a given [UserId].
    pub fn set_user_nicknames(&mut self, user: &UserId, nicks: &[&str]) {
        if nicks.is_empty() {
            self.user_specific_nicknames.remove(&user.to_string());
        } else {
            self.user_specific_nicknames.insert(
                user.to_string(),
                nicks.iter().map(|s| s.to_string()).collect(),
            );
        }
    }

    /// Select a nickname for the given [UserId], or [None] if the user is excluded.
    pub fn get_nickname_for_user(&self, user: &UserId) -> Option<String> {
        self.user_specific_nicknames
            .get(&user.to_string())
            .map(|n| n.choose(&mut rand::thread_rng()))
            .unwrap_or_default()
            .map(|s| s.to_string())
    }

    /// Select a [UserId] to change the nickname of.
    pub fn get_random_user(&self) -> Option<UserId> {
        self.user_specific_nicknames
            .keys()
            .choose(&mut rand::thread_rng())
            .map(|id| UserId::new(u64::from_str(id).unwrap()))
    }

    /// Set the complaints channel.
    pub fn set_complaints_channel(&mut self, channel: Option<ChannelId>) {
        self.complaint_channel = channel;
    }

    /// Get the complaints channel, if set.
    pub fn complaints_channel(&self) -> Option<ChannelId> {
        self.complaint_channel
    }

    /// Set title override.
    pub fn set_title_override(&mut self, title_override: Option<String>) {
        self.title_override = title_override;
    }

    /// Get title, either [Self::title_override] or the default string.
    pub fn title(&self) -> String {
        if let Some(title) = &self.title_override {
            title.clone()
        } else {
            "Bot demands new nickname".to_string()
        }
    }
}

#[async_trait]
impl Subsystem for NicknameLottery {
    fn generate_commands(&self) -> Vec<Command<'static>> {
        vec![Command::new(
            "nickname_lottery",
            "Controls for the nickname lottery.",
            PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
            None,
        )
        .add_variant(
            Command::new(
                "set_nicknames",
                "Set the list of nicknames that can be applied to a specific user.",
                PermissionType::ServerPerms(Permissions::MANAGE_NICKNAMES),
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async move {
                        let user = get_param!(params, User, "user");
                        let user = command.data.resolved.users.get(user).unwrap();
                        let guild_id = command.guild_id.unwrap();

                        let data = crate::acquire_data_handle!(read ctx);
                        let guild = get_guild(&data, &guild_id).unwrap();
                        let nickname_lottery_data = guild.nickname_lottery_data();

                        info!(
                            "[Guild: {}] Updating nickname list for {} ({})",
                            guild_id, user.name, user.id
                        );

                        let old_nicknames = nickname_lottery_data.user_nicknames_string(&user.id);

                        let input_nicks = serenity::builder::CreateInputText::new(
                            serenity::all::InputTextStyle::Paragraph,
                            "Nicknames list",
                            "nicknames_list",
                        )
                        .placeholder("List of nicknames, each on a new line (or blank to unset).")
                        .required(false)
                        .value(old_nicknames);
                        crate::drop_data_handle!(data);

                        let components =
                            vec![serenity::all::CreateActionRow::InputText(input_nicks)];

                        command
                            .create_response(
                                &ctx.http(),
                                serenity::all::CreateInteractionResponse::Modal(
                                    CreateModal::new(
                                        user.id.to_string() + "_set_nicknames",
                                        format!("{}'s nicknames", user.name),
                                    )
                                    .components(components),
                                ),
                            )
                            .await?;

                        let userid = user.id;
                        // collect the submitted data
                        if let Some(int) = serenity::collector::ModalInteractionCollector::new(ctx)
                            .filter(move |int| {
                                int.data.custom_id == userid.to_string() + "_set_nicknames"
                            })
                            .timeout(Duration::new(300, 0))
                            .await
                        {
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&guild_id.clone());
                            let nickname_lottery_data = guild.nickname_lottery_data_mut();

                            let inputs: Vec<_> = int
                                .data
                                .components
                                .iter()
                                .flat_map(|r| r.components.iter())
                                .collect();

                            for input in inputs.iter() {
                                if let serenity::all::ActionRowComponent::InputText(it) = input {
                                    if it.custom_id == "nicknames_list" {
                                        if let Some(it) = &it.value {
                                            nickname_lottery_data.set_user_nicknames(
                                            &user.id,
                                            &NicknameLotteryGuildData::deconstruct_nickname_string(
                                                &it.clone(),
                                            )
                                            .iter()
                                            .map(|n| n.as_str())
                                            .collect::<Vec<&str>>(),
                                        );
                                        }
                                    }
                                }
                            }

                            config.save();

                            // it's now safe to close the modal, so send a response to it
                            int.create_response(
                                &ctx.http(),
                                serenity::all::CreateInteractionResponse::Acknowledge,
                            )
                            .await?;
                        }

                        Ok(None)
                    })
                })),
            )
            .add_option(crate::Option::new(
                "user",
                "The user to set the nickname list for.",
                OptionType::User,
                true,
            )),
        )
        .add_variant(
            Command::new(
                "configure_announcements",
                "Configure announcements when the bot fails to change a user's nickname.",
                PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
                Some(Box::new(move |ctx, command, params| {
                    Box::pin(async {
                        // Set announcement channel if it's been supplied.
                        if let Some(channel_opt) = params.iter().find(|opt| opt.name == "channel") {
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            if let CommandDataOptionValue::Channel(channel) = &channel_opt.value {
                                let channel = channel.to_channel(&ctx.http()).await?;
                                guild
                                    .nickname_lottery_data_mut()
                                    .set_complaints_channel(Some(channel.id()));
                                config.save();
                            }
                        };

                        // Set title override if it's been supplied.
                        if let Some(title_opt) =
                            params.iter().find(|opt| opt.name == "title_override")
                        {
                            let mut data = crate::acquire_data_handle!(write ctx);
                            let config = data.get_mut::<Config>().unwrap();
                            let guild = config.guild_mut(&command.guild_id.unwrap());
                            let lottery_data = guild.nickname_lottery_data_mut();
                            if let CommandDataOptionValue::String(title_override) = &title_opt.value
                            {
                                lottery_data.set_title_override(Some(title_override.to_owned()));
                                config.save();
                            }
                        };

                        let data = crate::acquire_data_handle!(read ctx);
                        let guild = get_guild(&data, &command.guild_id.unwrap());
                        let lottery_data = &guild.unwrap().nickname_lottery_data();
                        let resp = format!(
                            "**Nickname lottery complaints channel updated!**
Channel: {}
Title text: {}",
                            lottery_data
                                .complaints_channel()
                                .unwrap()
                                .to_channel(&ctx.http())
                                .await?,
                            lottery_data.title()
                        );
                        Ok(Some(ActionResponse::new(create_raw_embed(resp), true)))
                    })
                })),
            )
            .add_option(crate::command::Option::new(
                "channel",
                "The channel to announce timeouts in.",
                OptionType::Channel(Some(vec![ChannelType::Text])),
                false,
            ))
            .add_option(crate::command::Option::new(
                "title_override",
                "Text to prepend before the timeout counter message.",
                OptionType::StringInput(None, None),
                false,
            )),
        )
        .add_variant(Command::new(
            "stop_announcements",
            "Stop all announcements. Unsets all configuration values.",
            PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
            Some(Box::new(move |ctx, command, _params| {
                Box::pin(async {
                    let mut data = crate::acquire_data_handle!(write ctx);
                    let config = data.get_mut::<Config>().unwrap();
                    let guild = config.guild_mut(&command.guild_id.unwrap());
                    let lottery_data = guild.nickname_lottery_data_mut();
                    lottery_data.set_complaints_channel(None);
                    lottery_data.set_title_override(None);
                    config.save();
                    crate::drop_data_handle!(data);

                    Ok(Some(ActionResponse::new(
                        create_raw_embed("Announcements have been uninitialised."),
                        true,
                    )))
                })
            })),
        ))]
    }
}

impl NicknameLottery {
    pub async fn guild_init(ctx: Context, g: Guild) {
        // between 30 minutes and 5 days
        let between = rand::distributions::Uniform::from(REFRESH_INTERVAL.0..REFRESH_INTERVAL.1);
        loop {
            let now = chrono::Utc::now();
            if cfg!(not(debug_assertions)) {
                let mut tts = Duration::from_secs(between.sample(&mut rand::thread_rng()));
                // It's April Fool's! Force the minimum refresh interval.
                if now.month() == 4 && now.day() == 1 {
                    tts = Duration::from_secs(1_800);
                } else if now.month() < 4 {
                    let ctts = chrono::Duration::from_std(tts);
                    match ctts {
                        Ok(ctts) => {
                            if (now + ctts).month() >= 4 {
                                // Current reset timer will either cross into, or completely skip, April Fool's.
                                // Clamp to time until April Fool's.
                                tts = match chrono::Utc
                                    .with_ymd_and_hms(now.year(), 4, 1, 0, 0, 0)
                                    .unwrap()
                                    .signed_duration_since(now)
                                    .to_std()
                                {
                                    Ok(tts) => tts,
                                    Err(e) => {
                                        #[cfg(feature = "events")]
                                        notify_subscribers(
                                            &ctx,
                                            Event::Error,
                                            &format!(
                                                "**[Guild: {}] Error calculating time until next nickname change:**
{e}

_Nickname changes are disabled for this guild until next initialisation._",
                                                g.id
                                            ),
                                        )
                                        .await;
                                        panic!("OutOfRangeError during reset time calculation.");
                                    }
                                };
                            }
                        }
                        Err(e) => {
                            #[cfg(feature = "events")]
                            notify_subscribers(
                                &ctx,
                                Event::Error,
                                &format!(
                                    "**[Guild: {}] Error calculating time until next nickname change:**
{e}

_Nickname changes are disabled for this guild until next initialisation._",
                                    g.id
                                ),
                            )
                            .await;
                            panic!("OutOfRangeError during reset time calculation.");
                        }
                    }
                }
                info!(
                    "[Guild: {}] Next nickname change in {} minutes.",
                    g.id,
                    (tts.as_secs() / 60)
                );
                tokio::time::sleep(tts).await;
            } else {
                info!(
                    "[Guild: {}] Running nickname lottery immediately once, for debugging.",
                    g.id
                );
            }
            // Time to update a user's nickname!
            let data = crate::acquire_data_handle!(read ctx);
            if let Some(guild) = get_guild(&data, &g.id) {
                let lottery_data = guild.nickname_lottery_data();
                if let Some(user) = lottery_data.get_random_user() {
                    if let Ok(member) = g.member(&ctx.http(), user).await {
                        let user = &member.user;
                        if let Some(mut new_nick) = lottery_data.get_nickname_for_user(&user.id) {
                            let old_nick = member.display_name();
                            // If feature `stream-indicator` is enabled, we want to preserve any applied streaming prefix, in case we're changing the nickname mid-stream.
                            #[cfg(feature = "stream-indicator")]
                            if old_nick
                                .starts_with(crate::subsystems::stream_indicator::STREAMING_PREFIX)
                            {
                                new_nick = crate::subsystems::stream_indicator::STREAMING_PREFIX
                                    .to_string()
                                    + &new_nick;
                            }
                            info!(
                                "[Guild: {}] Updating {}'s nickname to {} (current: {})",
                                &g.id, &user.id, &new_nick, &old_nick
                            );
                            if let Err(e) = g
                                .edit_member(
                                    &ctx.http(),
                                    user.id,
                                    serenity::all::EditMember::new().nickname(&new_nick),
                                )
                                .await
                            {
                                if let Some(channel_id) = lottery_data.complaints_channel() {
                                    let channel = match channel_id.to_channel(&ctx.http()).await {
                                        Ok(channel) => channel.guild(),
                                        Err(_) => None,
                                    };
                                    if let Some(channel) = channel {
                                        channel
                                            .send_message(
                                                &ctx.http(),
                                                create_embed(format!(
                                                    "**{}**
{} won/lost the lottery! From now on, they are to be named: `{}`",
                                                    lottery_data.title(),
                                                    user.mention(),
                                                    new_nick,
                                                )),
                                            )
                                            .await
                                            .unwrap();
                                    } else {
                                        #[cfg(feature = "events")]
                                        notify_subscribers_with_handle(
                                            &ctx,
                                            &data,
                                            Event::Error,
                                            &format!(
                                                "**[Guild: {}] Invalid complaints channel.**",
                                                g.id,
                                            ),
                                        )
                                        .await;
                                        error!("[Guild: {}] Invalid complaints channel.", g.id);
                                        continue;
                                    }
                                }
                                warn!(
                                    "[Guild: {}] Error changing {}'s nickname:
{e}",
                                    g.id, user.id
                                );
                            }
                        }
                    }
                }
            }
            if cfg!(debug_assertions) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use serenity::model::prelude::UserId;

    use super::NicknameLotteryGuildData;

    #[test]
    fn test_nickname_strings() {
        assert_eq!(
            NicknameLotteryGuildData::construct_nickname_string(
                &NicknameLotteryGuildData::deconstruct_nickname_string(
                    &"Nick1
Nick number 2    
     Nick_numerus_tres

A very long nickname that exceeds 30 characters
etc"
                )
            ),
            "Nick1
Nick number 2
Nick_numerus_tres
A very long nickname that exce
etc"
        )
    }

    #[test]
    fn test_setting_and_selecting_nicknames() {
        let users = [UserId::from(0), UserId::from(1)];
        let mut data: NicknameLotteryGuildData = NicknameLotteryGuildData::default();
        assert_eq!(data.get_nickname_for_user(&users[0]), None);
        assert_eq!(data.get_nickname_for_user(&users[1]), None);
        data.set_user_nicknames(&users[0], &["user0"]);
        data.set_user_nicknames(&users[1], &["user1"]);
        assert_eq!(
            data.get_nickname_for_user(&users[0]),
            Some("user0".to_string())
        );
        assert_eq!(
            data.get_nickname_for_user(&users[1]),
            Some("user1".to_string())
        );
        data.set_user_nicknames(&users[0], &[]);
        assert_eq!(data.get_nickname_for_user(&users[0]), None);
        assert_eq!(
            data.get_nickname_for_user(&users[1]),
            Some("user1".to_string())
        );
    }

    #[test]
    fn select_random_user() {
        let users = [UserId::from(0)];
        let mut data: NicknameLotteryGuildData = NicknameLotteryGuildData::default();
        assert_eq!(data.get_random_user(), None);
        data.set_user_nicknames(&users[0], &["user0"]);
        assert_eq!(data.get_random_user(), Some(users[0].clone()));
        data.set_user_nicknames(&users[0], &[]);
        assert_eq!(data.get_random_user(), None);
    }
}
