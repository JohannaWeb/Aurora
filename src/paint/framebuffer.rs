use crate::layout::Rect;
use std::fmt::{self, Display, Formatter};

use super::{CELL_HEIGHT_PX, CELL_WIDTH_PX};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameBuffer {
    width: usize,
    height: usize,
    cells: Vec<char>,
}

impl FrameBuffer {
    pub(super) fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![' '; width * height],
        }
    }

    pub(super) fn set(&mut self, x: usize, y: usize, value: char) {
        if x < self.width && y < self.height {
            self.cells[y * self.width + x] = value;
        }
    }

    pub(super) fn fill_rect(&mut self, rect: Rect, value: char) {
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        let x1 = ((rect.x + rect.width) / CELL_WIDTH_PX).ceil().max(0.0) as usize;
        let y1 = ((rect.y + rect.height) / CELL_HEIGHT_PX).ceil().max(0.0) as usize;

        for y in y0..y1.min(self.height) {
            for x in x0..x1.min(self.width) {
                self.set(x, y, value);
            }
        }
    }

    pub(super) fn draw_text(&mut self, rect: Rect, text: &str) {
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        if y0 >= self.height {
            return;
        }

        for (offset, ch) in text.chars().enumerate() {
            let x = x0 + offset;
            if x >= self.width {
                break;
            }
            self.set(x, y0, ch);
        }
    }

    pub(super) fn draw_outline(
        &mut self,
        rect: Rect,
        corner: char,
        horizontal: char,
        vertical: char,
    ) {
        let x0 = (rect.x / CELL_WIDTH_PX).floor().max(0.0) as usize;
        let y0 = (rect.y / CELL_HEIGHT_PX).floor().max(0.0) as usize;
        let x1 = edge_cell(rect.x + rect.width, CELL_WIDTH_PX).max(x0);
        let y1 = edge_cell(rect.y + rect.height, CELL_HEIGHT_PX).max(y0);

        if x0 >= self.width || y0 >= self.height || x1 < x0 || y1 < y0 {
            return;
        }

        for x in x0..=x1.min(self.width - 1) {
            self.set(x, y0, horizontal);
            if y1 < self.height {
                self.set(x, y1, horizontal);
            }
        }
        for y in y0..=y1.min(self.height - 1) {
            self.set(x0, y, vertical);
            if x1 < self.width {
                self.set(x1, y, vertical);
            }
        }

        self.set(x0, y0, corner);
        if x1 < self.width {
            self.set(x1, y0, corner);
        }
        if y1 < self.height {
            self.set(x0, y1, corner);
        }
        if x1 < self.width && y1 < self.height {
            self.set(x1, y1, corner);
        }
    }

    pub(super) fn draw_label_at_cell(&mut self, cell_x: usize, cell_y: usize, label: &str) {
        if cell_y >= self.height {
            return;
        }

        for (offset, ch) in label.chars().enumerate() {
            let x = cell_x + offset;
            if x >= self.width {
                break;
            }
            self.set(x, cell_y, ch);
        }
    }
}

impl Display for FrameBuffer {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        for row in self.cells.chunks(self.width) {
            let line = row.iter().collect::<String>();
            writeln!(f, "{}", line.trim_end_matches(' '))?;
        }
        Ok(())
    }
}

fn edge_cell(value: f32, cell_size: f32) -> usize {
    let cell = value / cell_size;
    if cell.fract() > 0.0 {
        cell.ceil() as usize
    } else {
        cell as usize
    }
    .saturating_sub(1)
}
