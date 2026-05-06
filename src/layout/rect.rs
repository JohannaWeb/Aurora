use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,

    pub y: f32,

    pub width: f32,

    pub height: f32,
}

impl Display for Rect {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[x: {:.0}, y: {:.0}, w: {:.0}, h: {:.0}]",
            self.x, self.y, self.width, self.height
        )
    }
}
