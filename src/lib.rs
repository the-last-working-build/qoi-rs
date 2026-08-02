mod decode;
mod error;
mod header;
mod types;

pub use decode::decode;
pub use error::DecodeError;
pub use types::{Channels, ColorSpace, DecodedImage, ImageDesc};

pub fn inspect_header(input: &[u8]) -> Result<ImageDesc, DecodeError> {
    header::parse_header(input)
}
