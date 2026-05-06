//! CPU/ASCII painter for layout debugging and tests.

mod debug;
mod elements;
mod fill;
mod framebuffer;
mod painter;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use debug::{DebugFrame, DebugPainter};
pub use framebuffer::FrameBuffer;
pub use painter::Painter;

const CELL_WIDTH_PX: f32 = 6.0;
const CELL_HEIGHT_PX: f32 = 10.0;
