mod error;
mod header;
mod types;

pub use error::DecodeError;
pub use types::{Channels, ColorSpace, ImageDesc};

pub fn inspect_header(input: &[u8]) -> Result<ImageDesc, DecodeError> {
    header::parse_header(input)
}
