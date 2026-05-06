#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl EdgeSizes {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MarginValue {
    Px(f32),
    Auto,
}

impl MarginValue {
    pub fn to_px(self) -> f32 {
        match self {
            Self::Px(px) => px,
            Self::Auto => 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Margin {
    pub top: f32,
    pub right: MarginValue,
    pub bottom: f32,
    pub left: MarginValue,
}

impl Margin {
    pub fn zero() -> Self {
        Self {
            top: 0.0,
            right: MarginValue::Px(0.0),
            bottom: 0.0,
            left: MarginValue::Px(0.0),
        }
    }

    pub fn horizontal(&self) -> f32 {
        self.left.to_px() + self.right.to_px()
    }

    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Block,
    Inline,
    InlineBlock,
    Flex,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JustifyContent {
    #[default]
    FlexStart,
    Center,
    FlexEnd,
    SpaceBetween,
    SpaceAround,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlignItems {
    #[default]
    Stretch,
    FlexStart,
    Center,
    FlexEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxSizing {
    #[default]
    ContentBox,
    BorderBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WhiteSpace {
    #[default]
    Normal,
    NoWrap,
}
