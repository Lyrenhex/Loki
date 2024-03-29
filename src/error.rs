use std::fmt::Display;

#[derive(Debug)]
pub enum Error {
    InvalidChannel,
    InvalidUser,
    InvalidEvent(String),
    InvalidParam(String),
    MissingActionRoutine,
    SerenityError(serenity::Error),
}

impl From<serenity::Error> for Error {
    fn from(e: serenity::Error) -> Self {
        Self::SerenityError(e)
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidChannel => write!(
                f,
                "**Error: Invalid channel**
Are you sure it's the correct type of channel, and that I have \
access to it?"
            ),
            Self::InvalidUser => write!(f, "**Error: Invalid user**"),
            Self::InvalidEvent(s) => write!(
                f,
                "**Error: Invalid event**
{s}"
            ),
            Self::InvalidParam(s) => write!(
                f,
                "**Error: Invalid Parameter**
Either Discord has failed to provide a parameter marked required, a \
parameter isn't marked required when it should be, or the parameter's
type was different than expected:
`{s}`"
            ),
            Self::MissingActionRoutine => write!(
                f,
                "**Error: Missing Action Routine**
Whoops! This is _almost certainly_ a development oversight...
Badger the bot manager about it."
            ),
            Self::SerenityError(e) => match e {
                serenity::Error::Http(e) => match &e {
                    serenity::all::HttpError::UnsuccessfulRequest(resp) => {
                        if resp.status_code == serenity::http::StatusCode::FORBIDDEN {
                            write!(
                                f,
                                "**Serenity HTTP Error: {}**
_Do I have all required permissions to all appropriate channels?_
I require:
- `Send Messages` in any channel you want me to operate in.
- `Read Message History` and `Add Reactions` in the memes channel.
- `Manage Nicknames` to automatically identify people who are \
livestreaming.

```json
{:?}
```",
                                resp.status_code, resp.error
                            )
                        } else {
                            write!(
                                f,
                                "**Serenity HTTP Error: Unsuccessful request ({})**
```json
{:?}
```",
                                resp.status_code, resp.error
                            )
                        }
                    }
                    e => write!(
                        f,
                        "**Serenity HTTP Error**
```json
{e:?}
```"
                    ),
                },
                e => write!(
                    f,
                    "**Unhandled Serenity error...**
Well, something's clearly gone wrong.
```rust
{e:?}
```"
                ),
            },
        }
    }
}
