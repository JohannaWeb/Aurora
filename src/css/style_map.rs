use std::collections::BTreeMap;

use super::length::parse_length_px;
use super::shorthand::{
    parse_border_color_shorthand, parse_border_width_shorthand, parse_box_shorthand,
    parse_margin_shorthand, parse_margin_value,
};
use super::{
    AlignItems, BoxSizing, DisplayMode, EdgeSizes, FlexDirection, JustifyContent, Margin,
    TextAlign, WhiteSpace,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StyleMap(pub(super) BTreeMap<String, String>);

impl StyleMap {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn display_mode(&self) -> DisplayMode {
        match self.0.get("display").map(String::as_str) {
            Some("inline") | Some("inline-block") => DisplayMode::Inline,
            Some("flex") => DisplayMode::Flex,
            Some("none") => DisplayMode::None,
            _ => DisplayMode::Block,
        }
    }

    pub fn flex_direction(&self) -> FlexDirection {
        match self.0.get("flex-direction").map(String::as_str) {
            Some("column") => FlexDirection::Column,
            _ => FlexDirection::Row,
        }
    }

    pub fn justify_content(&self) -> JustifyContent {
        match self.0.get("justify-content").map(String::as_str) {
            Some("center") => JustifyContent::Center,
            Some("flex-end") => JustifyContent::FlexEnd,
            Some("space-between") => JustifyContent::SpaceBetween,
            Some("space-around") => JustifyContent::SpaceAround,
            _ => JustifyContent::FlexStart,
        }
    }

    pub fn align_items(&self) -> AlignItems {
        match self.0.get("align-items").map(String::as_str) {
            Some("flex-start") => AlignItems::FlexStart,
            Some("center") => AlignItems::Center,
            Some("flex-end") => AlignItems::FlexEnd,
            _ => AlignItems::Stretch,
        }
    }

    pub fn flex_wrap(&self) -> bool {
        matches!(self.0.get("flex-wrap").map(String::as_str), Some("wrap"))
    }

    pub fn gap_px(&self) -> f32 {
        self.get("column-gap")
            .and_then(parse_length_px)
            .or_else(|| self.get("gap").and_then(parse_length_px))
            .unwrap_or(0.0)
    }

    pub fn text_align(&self) -> TextAlign {
        match self.0.get("text-align").map(String::as_str) {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Right,
            _ => TextAlign::Left,
        }
    }

    pub fn white_space(&self) -> WhiteSpace {
        match self.0.get("white-space").map(String::as_str) {
            Some("nowrap") => WhiteSpace::NoWrap,
            _ => WhiteSpace::Normal,
        }
    }

    pub fn box_sizing(&self) -> BoxSizing {
        match self.0.get("box-sizing").map(String::as_str) {
            Some("border-box") => BoxSizing::BorderBox,
            _ => BoxSizing::ContentBox,
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    pub fn set(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0.insert(name.into(), value.into());
    }

    pub fn margin(&self) -> Margin {
        let mut margin = parse_margin_shorthand(self.get("margin"));
        if let Some(top) = self.get("margin-top").and_then(parse_length_px) {
            margin.top = top;
        }
        if let Some(right) = self.get("margin-right") {
            margin.right = parse_margin_value(right);
        }
        if let Some(bottom) = self.get("margin-bottom").and_then(parse_length_px) {
            margin.bottom = bottom;
        }
        if let Some(left) = self.get("margin-left") {
            margin.left = parse_margin_value(left);
        }
        margin
    }

    pub fn padding(&self) -> EdgeSizes {
        self.edge_sizes("padding")
    }

    pub fn border_width(&self) -> EdgeSizes {
        let mut edges = parse_box_shorthand(self.get("border-width"));
        if edges == EdgeSizes::zero() {
            edges = parse_border_width_shorthand(self.get("border"));
        }
        edges.top = self.length_or("border-top-width", edges.top);
        edges.right = self.length_or("border-right-width", edges.right);
        edges.bottom = self.length_or("border-bottom-width", edges.bottom);
        edges.left = self.length_or("border-left-width", edges.left);
        edges
    }

    pub fn background_color(&self) -> Option<&str> {
        self.get("background-color")
            .or_else(|| self.get("background"))
    }

    pub fn border_color(&self) -> Option<&str> {
        self.get("border-color")
            .or_else(|| parse_border_color_shorthand(self.get("border")))
    }

    pub fn width_px(&self) -> Option<f32> {
        self.get("width").and_then(parse_length_px)
    }

    pub fn height_px(&self) -> Option<f32> {
        self.get("height").and_then(parse_length_px)
    }

    pub fn min_width_px(&self) -> Option<f32> {
        self.get("min-width").and_then(parse_length_px)
    }

    pub fn max_width_px(&self) -> Option<f32> {
        self.get("max-width").and_then(parse_length_px)
    }

    pub fn min_height_px(&self) -> Option<f32> {
        self.get("min-height").and_then(parse_length_px)
    }

    pub fn max_height_px(&self) -> Option<f32> {
        self.get("max-height").and_then(parse_length_px)
    }

    pub fn font_size_px(&self) -> Option<f32> {
        self.get("font-size").and_then(parse_length_px)
    }

    pub(super) fn edge_sizes(&self, prefix: &str) -> EdgeSizes {
        let mut edges = parse_box_shorthand(self.get(prefix));
        edges.top = self.length_or(format!("{prefix}-top").as_str(), edges.top);
        edges.right = self.length_or(format!("{prefix}-right").as_str(), edges.right);
        edges.bottom = self.length_or(format!("{prefix}-bottom").as_str(), edges.bottom);
        edges.left = self.length_or(format!("{prefix}-left").as_str(), edges.left);
        edges
    }

    pub(super) fn length_or(&self, property: &str, fallback: f32) -> f32 {
        self.get(property)
            .and_then(parse_length_px)
            .unwrap_or(fallback)
    }
}
