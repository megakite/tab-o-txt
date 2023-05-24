use std::{
    collections::HashMap,
    fs::File,
    io::{self, Read},
};

use unicode_width::UnicodeWidthStr;

use crate::editor::Config;

pub struct Sheet {
    units: HashMap<(usize, usize), Unit>,
    /// Size of the sheet. Represented in `(col, row)`.
    size: (usize, usize),
    tab_size: usize,
    widths: Vec<usize>,
    accum_widths: Vec<usize>,
}

impl Sheet {
    pub fn new() -> Self {
        Self {
            units: HashMap::new(),
            size: (1, 1),
            tab_size: 8,
            widths: vec![0],
            accum_widths: vec![0, 1],
        }
    }

    pub fn from(config: Config) -> Self {
        Self {
            units: HashMap::new(),
            size: (1, 1),
            tab_size: config.tab_size,
            widths: vec![0],
            accum_widths: vec![0, 1],
        }
    }

    pub fn from_file(path: &str, config: Config) -> io::Result<Self> {
        let mut buf = String::new();
        File::options()
            .read(true)
            .write(true)
            .open(path)?
            .read_to_string(&mut buf)?;

        Ok(Self::from_str(&buf, config))
    }

    pub fn tab_size(&self) -> usize {
        self.tab_size
    }

    pub fn size(&self) -> (usize, usize) {
        self.size
    }

    pub fn width_at(&self, index: usize) -> Option<&usize> {
        self.widths.get(index)
    }

    pub fn accum_width_at(&self, index: usize) -> Option<&usize> {
        self.accum_widths.get(index)
    }

    pub fn content_at(&self, pos: (usize, usize)) -> Option<&str> {
        self.units.get(&pos).map(|u| u.content.as_str())
    }

    pub fn get_display_pos(&self, pos: (usize, usize), corner: (usize, usize)) -> (usize, usize) {
        (
            self.accum_widths[pos.0].saturating_sub(self.accum_widths[corner.0]) * self.tab_size,
            pos.1.saturating_sub(corner.1),
        )
    }

    fn from_str(buf: &str, config: Config) -> Self {
        let widths = Self::get_widths(buf, config.tab_size);
        let mut accum_widths = vec![0];
        for i in 0..widths.len() {
            accum_widths.push(widths[i] + accum_widths[i]);
        }

        let mut units_map = HashMap::new();

        let mut row: usize = 0;
        for line in buf.lines() {
            let mut col: usize = 0;
            let mut items = line.split('\t');
            while let Some(s) = items.next() {
                if !s.is_empty() {
                    units_map.insert((col, row), Unit::from(s));
                }

                let width = Self::measure_width(s, config.tab_size);
                let diff = widths[col].saturating_sub(width);
                if diff > 0 {
                    items.nth(diff - 1);
                }

                col += 1;
            }

            row += 1;
        }

        Self {
            units: units_map,
            size: (widths.len(), row),
            tab_size: config.tab_size,
            widths,
            accum_widths,
        }
    }

    pub fn edit(&mut self, pos: (usize, usize), buf: &str) {
        if buf.is_empty() {
            self.units.remove(&pos);

            if self.is_col_empty(pos.0) {
                self.remove_col(pos.0);
            }
            if self.is_row_empty(pos.1) {
                self.remove_row(pos.1);
            }
        } else {
            self.units
                .entry(pos)
                .and_modify(|mut unit| {
                    unit.content = buf.trim().to_owned();
                })
                .or_insert_with(|| Unit::from(buf.trim()));

            self.size.0 = self.size.0.max(pos.0 + 1);
            self.size.1 = self.size.1.max(pos.1 + 1);

            if let Some(&n) = self.widths.get(pos.0) {
                let width = self.get_col_width(pos.0).unwrap();
                if n != width {
                    self.widths[pos.0] = width;
                }
            } else {
                let width = Self::measure_width(buf, self.tab_size);
                self.widths.push(width);
            }
        }

        let mut new_accum_widths = vec![0];
        for i in 0..self.widths.len() {
            new_accum_widths.push(self.widths[i] + new_accum_widths[i]);
        }

        self.accum_widths = new_accum_widths;
    }

    /// Measures total width of the column of `index`. Returns `None` if specified column is empty.
    fn get_col_width(&self, index: usize) -> Option<usize> {
        self.units
            .iter()
            .filter(|u| u.0 .0 == index)
            .map(|u| Sheet::measure_width(&u.1.content, self.tab_size))
            .max()
    }

    /// Removes the columns of `index`. Will do nothing if `index` is out of bounds.
    fn remove_col(&mut self, index: usize) {
        if !(index < self.size.0) {
            return;
        }

        for col in index + 1..self.size.0 {
            for row in 0..self.size.1 {
                if let Some(v) = self.units.remove(&(col, row)) {
                    self.units.insert((col - 1, row), v);
                }
            }
        }

        self.widths.remove(index);
        self.size.0 = self.widths.len();
    }

    /// Removes the row of `index`. Will do nothing if `index` is out of bounds.
    fn remove_row(&mut self, index: usize) {
        if !(index < self.size.1) {
            return;
        }

        for col in 0..self.size.0 {
            for row in index..self.size.1 {
                if let Some(v) = self.units.remove(&(col, row)) {
                    self.units.insert((col, row - 1), v);
                }
            }
        }

        self.size.1 -= 1;
    }

    /// Checks if the column of `index` is empty.
    fn is_col_empty(&self, index: usize) -> bool {
        for row in 0..self.size.0 {
            if self.units.contains_key(&(index, row)) {
                return false;
            }
        }

        true
    }

    fn is_row_empty(&self, index: usize) -> bool {
        for col in 0..self.size.1 {
            if self.units.contains_key(&(col, index)) {
                return false;
            }
        }

        true
    }

    fn measure_width(content: &str, tab_size: usize) -> usize {
        UnicodeWidthStr::width(content) / tab_size + 1
    }

    /// Gets column widths from given string slice.
    fn get_widths(content: &str, tab_size: usize) -> Vec<usize> {
        let mut widths: Vec<usize> = vec![];

        for line in content.lines() {
            let mut index: usize = 0;
            let mut items = line.split('\t').peekable();

            'outer: while let Some(item) = items.next() {
                let mut width: usize = UnicodeWidthStr::width(item) / tab_size + 1;

                while let Some(&following) = items.peek() {
                    if following.is_empty() {
                        items.next();
                        width += 1;
                    } else {
                        break;
                    }
                }

                while let Some(&prev_width) = widths.get(index) {
                    let (value, overflow) = width.overflowing_sub(prev_width);

                    if !overflow {
                        if value > 0 {
                            if index == widths.len() - 1 {
                                widths[index] = width;
                            } else {
                                width -= prev_width;
                                index += 1;
                            }
                        } else {
                            index += 1;
                            continue 'outer;
                        }
                    } else {
                        if items.peek().is_some() {
                            widths[index] = width;
                            widths.insert(index + 1, prev_width - width);
                        }
                        index += 1;
                        continue 'outer;
                    }
                }

                widths.push(width);
                index += 1;
            }
        }

        widths
    }
}

impl Default for Sheet {
    fn default() -> Self {
        Sheet::new()
    }
}

#[derive(Debug)]
pub struct Unit {
    content: String,
}

impl Unit {
    fn new() -> Self {
        Unit {
            content: String::new(),
        }
    }
}

impl From<&str> for Unit {
    fn from(value: &str) -> Self {
        Unit {
            content: value.to_owned(),
        }
    }
}

impl Default for Unit {
    fn default() -> Self {
        Self::new()
    }
}
