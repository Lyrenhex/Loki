use serde::{Deserialize, Serialize};

use crate::command::{Command, OptionType, PermissionType};

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Event {
    Startup,
}

// pub fn generate_command() -> Command<'static> {
//     Command::new(
//         "events",
//         "Manage subscriptions to notifications for specific bot events.",
//         PermissionType::Universal,
//         None,
//     )
//     .add_variant(
//         Command::new(
//             "subscribe",
//             "Subscribe to a bot event. Some events may be restricted.",
//             PermissionType::Universal,
//             None,
//         )
//         .add_option(crate::command::Option::new(
//             "channel",
//             "The channel which is to be used for memes.",
//             OptionType::StringSelect(&[]),
//             true,
//         )),
//     )
//     .add_variant(Command::new(
//         "unset_channel",
//         "Unsets the memes channel for this server, resetting the meme subsystem.",
//         PermissionType::ServerPerms(Permissions::MANAGE_CHANNELS),
//         Some(Box::new(move |ctx, command| {
//             Box::pin(async {
//                 let mut data = ctx.data.write().await;
//                 let config = data.get_mut::<Config>().unwrap();
//                 let channel = config
//                     .guild_mut(&command.guild_id.unwrap())
//                     .memes()
//                     .map(|memes| memes.channel());
//                 config
//                     .guild_mut(&command.guild_id.unwrap())
//                     .set_memes_channel(None);
//                 config.save();
//                 drop(data);
//                 let resp = "Memes channel unset.".to_string();
//                 create_response(&ctx.http, command, &resp).await;
//                 if let Some(channel) = channel {
//                     if let Some(channel) = channel.to_channel(&ctx.http).await?.guild() {
//                         channel
//                             .send_message(
//                                 &ctx.http,
//                                 create_embed(
//                                     "**Halt your memes!**
// I won't see them anymore. :("
//                                         .to_string(),
//                                 ),
//                             )
//                             .await?;
//                     }
//                 }
//                 Ok(())
//             })
//         })),
//     ))
// }
