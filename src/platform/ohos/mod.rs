//! HarmonyOS PC/2in1 platform implementation.

mod constants;
mod display;
mod keycodes;
mod lifecycle;
mod listen;
mod media;
mod result;
mod scroll;
mod simulate;
mod translate;

pub(crate) fn begin_relative_pointer_capture() -> crate::Result<()> {
    Err(crate::Error::NotSupported(
        "relative pointer capture is not supported by HarmonyOS Input Kit".into(),
    ))
}

pub(crate) fn end_relative_pointer_capture() -> crate::Result<()> {
    Ok(())
}

pub use display::*;
pub use listen::*;
pub use media::media_key;
pub use scroll::scroll;
pub use simulate::*;
