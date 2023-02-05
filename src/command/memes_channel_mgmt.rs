use serenity::model::{
    prelude::{
        command::CommandOptionType, interaction::application_command::CommandDataOptionValue,
    },
    Permissions,
};

use crate::{config::Config, COLOUR};

use super::{create_response, Command, PermissionType};

pub fn memes_channel_mgmt() -> Command<'static> {
    Command::new(
        "memes",
        "Configuration commands for the meme-voting system.",
        PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
        None,
    )
    .add_variant(
        Command::new(
            "set_channel",
            "Sets the memes channel for this server and initialises the meme subsystem.",
            PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
            Some(Box::new(move |ctx, command| {
                Box::pin(async {
                    let (channel_id, channel) =
                        if let Some(CommandDataOptionValue::Channel(channel)) =
                            &command.data.options[0].options[0].resolved
                        {
                            if let Some(channel) = channel.id.to_channel(&ctx.http).await?.guild() {
                                (channel.id, channel)
                            } else {
                                return Err(crate::Error::InvalidChannel);
                            }
                        } else {
                            return Err(crate::Error::InvalidChannel);
                        };
                    let mut data = ctx.data.write().await;
                    let config = data.get_mut::<Config>().unwrap();
                    let guild_config = config.guild_mut(&command.guild_id.unwrap());
                    guild_config.set_memes_channel(Some(channel_id));
                    let reset_time = guild_config.memes().unwrap().next_reset();
                    config.save();
                    drop(data);
                    let resp = format!("Memes channel set to {}.", channel);
                    channel
                        .send_message(&ctx.http, |m| {
                            m.embed(|e| {
                                e.description(format!(
                                    "**Post your best memes!**
Vote by reacting to your favourite memes.
The post with the most total reactions by {} wins!",
                                    reset_time.format(crate::DATE_FMT),
                                ))
                                .colour(COLOUR)
                            })
                        })
                        .await?;
                    create_response(&ctx.http, command, &resp).await;
                    Ok(())
                })
            })),
        )
        .add_option(
            super::Option::new(
                "channel",
                "The channel which is to be used for memes.",
                CommandOptionType::Channel,
                true,
            )
            .unwrap(),
        ),
    )
    .add_variant(Command::new(
        "unset_channel",
        "Unsets the memes channel for this server, resetting the meme subsystem.",
        PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
        Some(Box::new(move |ctx, command| {
            Box::pin(async {
                let mut data = ctx.data.write().await;
                let config = data.get_mut::<Config>().unwrap();
                let channel =
                    if let Some(memes) = config.guild_mut(&command.guild_id.unwrap()).memes() {
                        Some(memes.channel())
                    } else {
                        None
                    };
                config
                    .guild_mut(&command.guild_id.unwrap())
                    .set_memes_channel(None);
                config.save();
                drop(data);
                let resp = "Memes channel unset.".to_string();
                create_response(&ctx.http, command, &resp).await;
                if let Some(channel) = channel {
                    if let Some(channel) = channel.to_channel(&ctx.http).await?.guild() {
                        channel
                            .send_message(&ctx.http, |m| {
                                m.embed(|e| {
                                    e.description(
                                        "**Halt your memes!**
I won't see them anymore. :(",
                                    )
                                    .colour(COLOUR)
                                })
                            })
                            .await?;
                    }
                }
                Ok(())
            })
        })),
    ))
}
