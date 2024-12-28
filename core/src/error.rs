

#[derive(Debug)]
pub enum Error {
    IOError(std::io::Error),
    JsonError(json::JsonError),
    TimeError(std::time::SystemTimeError),
    OtherError(String),
}

impl std::error::Error for Error {}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) 
        -> std::fmt::Result
    { write!(f, "{:?}", self) }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::IOError(value)
    }
}
impl From<json::JsonError> for Error {
    fn from(value: json::JsonError) -> Self {
        Error::JsonError(value)
    }
}
impl From<std::time::SystemTimeError> for Error {
    fn from(value: std::time::SystemTimeError) -> Self {
        Error::TimeError(value)
    }
}
impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Error::OtherError(value.to_string())
    }
}
impl From<String> for Error {
    fn from(value: String) -> Self {
        Error::OtherError(value)
    }
}

