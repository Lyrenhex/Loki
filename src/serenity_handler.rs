use log::{error, info};
use serenity::{
    async_trait,
    model::prelude::{command::Command, interaction::Interaction, Activity, Ready},
    prelude::{Context, EventHandler},
};

/// Core implementation logic for [serenity] events.
pub struct SerenityHandler<'a> {
    commands: Vec<crate::command::Command<'a>>,
}

#[async_trait]
impl EventHandler for SerenityHandler<'_> {
    async fn ready(&self, ctx: Context, ready: Ready) {
        info!("Loki is connected as {}", ready.user.name);

        ctx.set_activity(Activity::playing("tricks")).await;

        // creates the global application commands
        self.create_commands(&ctx).await;
    }

    async fn interaction_create(&self, ctx: Context, interaction: Interaction) {
        if let Interaction::ApplicationCommand(mut command) = interaction {
            for cmd in self.commands.iter() {
                if cmd.name() == command.data.name {
                    if let Err(e) = cmd.run(&ctx, &mut command).await {
                        error!("Error running '{}': {e:?}", cmd.name());
                        todo!("error handling");
                    }
                    break;
                }
            }
        };
    }
}

impl<'a> SerenityHandler<'a> {
    /// Construct a new handler from a populated config.
    pub fn new(commands: Vec<crate::command::Command<'a>>) -> Self {
        Self { commands }
    }

    async fn create_commands(&self, ctx: &Context) -> Vec<Command> {
        Command::set_global_application_commands(&ctx.http, |mut commands| {
            for cmd in self.commands.iter() {
                commands = commands.create_application_command(|command| {
                    command.name(cmd.name()).description(cmd.description())
                })
            }
            commands
        })
        .await
        .expect("Failed to create command")
    }
}
