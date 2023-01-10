#[derive(Debug)]
pub enum Error {
    SerenityError(serenity::Error),
}

impl From<serenity::Error> for Error {
    fn from(e: serenity::Error) -> Self {
        Self::SerenityError(e)
    }
}
