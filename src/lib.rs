#![forbid(unsafe_code)]

mod decode;
mod encode;
mod error;
mod header;
mod types;

pub use decode::decode;
pub use encode::encode;
pub use error::{DecodeError, EncodeError};
pub use types::{Channels, ColorSpace, DecodedImage, ImageDesc};

pub fn inspect_header(input: &[u8]) -> Result<ImageDesc, DecodeError> {
    header::parse_header(input)
}
