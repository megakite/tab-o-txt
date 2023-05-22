use std::collections::HashMap;
use std::fs::File;
use std::io::{self, stdin, stdout, Read, Write};
use std::ops::Add;

use crossterm::cursor::MoveLeft;
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Print, ResetColor, SetAttribute};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, event, execute, terminal};

use unicode_width::UnicodeWidthStr;

pub struct Session {
    mode: Mode,
    file_path: Option<String>,
    sheet: Sheet,
    /// Current cursor position. Zero-indexed. Represented in `(col, row)`.
    pos: (usize, usize),
    /// From where the table starts to be drawn. Zero-indexed. Represented in `(col, row)`.
    corner: (usize, usize),
}

impl Session {
    pub fn new(config: Config, args: &[String]) -> io::Result<Self> {
        let mode = Mode::Navigate;
        let file_path = args.get(1).cloned();
        let sheet = match &file_path {
            Some(f) => Sheet::from_file(f, config)?,
            None => Sheet::new(config),
        };

        Ok(Self {
            mode,
            file_path,
            sheet,
            pos: (0, 0),
            corner: (0, 0),
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        execute!(stdout(), terminal::EnterAlternateScreen)?;

        loop {
            match self.mode {
                Mode::Navigate => {
                    terminal::enable_raw_mode()?;
                    self.navigate()?;
                }
                Mode::Edit => {
                    terminal::enable_raw_mode()?;
                    self.edit()?;
                }
                Mode::Command => {
                    terminal::disable_raw_mode()?;
                    self.command()?;
                }
                Mode::Quit => {
                    terminal::disable_raw_mode()?;
                    self.quit()?;

                    break;
                }
            }
        }

        Ok(())
    }

    fn navigate(&mut self) -> io::Result<()> {
        self.refresh()?;

        execute!(
            stdout(),
            cursor::MoveTo(
                ((self.sheet.accum_widths[self.pos.0] - self.sheet.accum_widths[self.corner.0])
                    * self.sheet.tab_size) as u16,
                (self.pos.1 - self.corner.1) as u16,
            )
        )?;

        if let Some(u) = self.sheet.units.get(&self.pos) {
            execute!(
                stdout(),
                SetAttribute(Attribute::Reverse),
                Print(&u.content),
                ResetColor,
            )?;
        };

        if let Event::Key(event) = event::read()? {
            match event {
                KeyEvent {
                    code: KeyCode::Up, ..
                }
                | KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::SHIFT,
                    ..
                } => {
                    self.move_cursor_by(0, -1)?;
                }
                KeyEvent {
                    code: KeyCode::Left,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::SHIFT,
                    ..
                } => {
                    self.move_cursor_by(-1, 0)?;
                }
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    self.move_cursor_by(0, 1)?;
                }
                KeyEvent {
                    code: KeyCode::Right,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Tab, ..
                } => {
                    self.move_cursor_by(1, 0)?;
                }
                KeyEvent {
                    code: KeyCode::PageDown,
                    ..
                } => {
                    self.move_cursor_by(0, (terminal::size().unwrap().1 - 1) as isize)?;
                }
                KeyEvent {
                    code: KeyCode::PageUp,
                    ..
                } => {
                    self.move_cursor_by(0, -((terminal::size().unwrap().1 - 1) as isize))?;
                }

                KeyEvent {
                    code: KeyCode::Char(':'),
                    ..
                } => {
                    self.mode = Mode::Command;
                }

                KeyEvent {
                    code: KeyCode::Esc, ..
                } => {
                    self.mode = Mode::Quit;
                }

                KeyEvent {
                    code: KeyCode::F(2),
                    ..
                } => {
                    self.mode = Mode::Edit;
                }

                _ => {
                    todo!()
                }
            }
        }

        Ok(())
    }

    fn move_cursor_by(&mut self, x: isize, y: isize) -> io::Result<()> {
        let size = terminal::size()?;

        self.pos.0 = self
            .pos
            .0
            .saturating_add_signed(x)
            .clamp(0, self.sheet.size.0);
        self.pos.1 = self
            .pos
            .1
            .saturating_add_signed(y)
            .clamp(0, self.sheet.size.1);

        if !is_in_offset_bounds(
            self.sheet.accum_widths[self.pos.0],
            self.sheet.accum_widths[self.corner.0],
            (size.0 as usize - 1) / self.sheet.tab_size,
        ) {
            self.corner.0 = self.corner.0.saturating_add_signed(x);
        }
        if !is_in_offset_bounds(self.pos.1, self.corner.1, size.1 as usize - 1) {
            self.corner.1 = self.corner.1.saturating_add_signed(y);
        }

        Ok(())
    }

    fn edit(&mut self) -> io::Result<()> {
        let mut buf = match self.sheet.units.get(&self.pos) {
            Some(unit) => unit.content.to_owned(),
            None => String::new(),
        };
        buf = read_line_initial_text(&buf)?;

        if buf.is_empty() {
            self.sheet.units.remove(&self.pos);
        } else {
            self.sheet
                .units
                .entry(self.pos)
                .and_modify(|mut unit| {
                    unit.content = buf.trim().to_owned();
                })
                .or_insert_with(|| Unit::from(buf.trim()));

            self.sheet.size.0 = self.sheet.size.0.max(self.pos.0 + 1);
            self.sheet.size.1 = self.sheet.size.1.max(self.pos.1 + 1);
        }

        if let Some(n) = self.sheet.widths.get_mut(self.pos.0) {
            let max_width_of_current_col = self
                .sheet
                .units
                .iter()
                .filter(|u| u.0 .0 == self.pos.0)
                .map(|u| UnicodeWidthStr::width(u.1.content.as_str()) / self.sheet.tab_size + 1)
                .max()
                .unwrap_or(1);
            if *n != max_width_of_current_col {
                *n = max_width_of_current_col;
            }
        } else {
            let width = UnicodeWidthStr::width(buf.as_str()) / self.sheet.tab_size + 1;
            self.sheet.widths.push(width);
        }

        let mut new_accum_widths = vec![0];
        for i in 0..self.sheet.widths.len() {
            new_accum_widths.push(self.sheet.widths[i] + new_accum_widths[i]);
        }

        self.sheet.accum_widths = new_accum_widths;

        self.mode = Mode::Navigate;

        Ok(())
    }

    fn command(&mut self) -> io::Result<()> {
        execute!(stdout(), cursor::MoveTo(0, terminal::size().unwrap().1 - 1))?;
        print!(":");
        stdout().flush()?;

        let mut command = String::new();
        stdin().read_line(&mut command)?;

        self.parse_command(command.trim())?;

        Ok(())
    }

    fn quit(&mut self) -> io::Result<()> {
        execute!(stdout(), terminal::LeaveAlternateScreen)?;

        Ok(())
    }

    fn print(&mut self) -> io::Result<()> {
        let size = terminal::size()?;

        for unit in &self.sheet.units {
            if !(is_in_offset_bounds(
                unit.0 .0,
                self.corner.0,
                (size.0 as usize - 1) / self.sheet.tab_size,
            ) && is_in_offset_bounds(unit.0 .1, self.corner.1, size.1 as usize - 1))
            {
                continue;
            }

            let col = self.sheet.accum_widths[unit.0 .0]
                .saturating_sub(self.sheet.accum_widths[self.corner.0])
                * self.sheet.tab_size;
            let row = unit.0 .1.saturating_sub(self.corner.1);

            execute!(
                stdout(),
                cursor::MoveTo(col as u16, row as u16),
                Print(&unit.1.content),
            )?;
        }

        Ok(())
    }

    fn refresh(&mut self) -> io::Result<()> {
        execute!(
            stdout(),
            // Clear(ClearType::All),
            Clear(ClearType::FromCursorUp),
            Clear(ClearType::CurrentLine),
            Clear(ClearType::FromCursorDown),
        )?;

        self.print()?;

        Ok(())
    }

    fn parse_command(&mut self, cmd: &str) -> io::Result<()> {
        let mut iter = cmd.chars();

        while let Some(c) = iter.next() {
            match c {
                'w' => {
                    self.save()?;
                    self.mode = Mode::Navigate;
                }
                'q' => {
                    self.mode = Mode::Quit;
                }
                _ => {
                    self.mode = Mode::Navigate;
                }
            }
        }

        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        let file_path = match &self.file_path {
            Some(fp) => fp.to_owned(),
            None => {
                let mut buf = String::new();
                stdin().read_to_string(&mut buf)?;
                buf
            }
        };
        let mut file = File::options().create(true).write(true).open(file_path)?;

        for row in 0..self.sheet.size.1 {
            let mut count: usize = 0;
            for col in 0..self.sheet.size.0 {
                if let Some(u) = self.sheet.units.get(&(col, row)) {
                    file.write_all(&b"\t".repeat(count))?;
                    file.write_all(u.content.as_bytes())?;

                    let width =
                        UnicodeWidthStr::width(u.content.as_str()) / self.sheet.tab_size + 1;
                    count = 1 + (self.sheet.widths[col] - width);
                } else {
                    count += self.sheet.widths[col];
                }
            }
            file.write_all(b"\n")?;
        }

        Ok(())
    }
}

pub struct Config {
    tab_size: usize,
}

impl Config {
    pub fn new() -> Self {
        Self { tab_size: 8 }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::new()
    }
}

struct Sheet {
    units: HashMap<(usize, usize), Unit>,
    /// Size of the sheet. Represented in `(col, row)`.
    size: (usize, usize),
    tab_size: usize,
    widths: Vec<usize>,
    accum_widths: Vec<usize>,
}

impl Sheet {
    fn new(config: Config) -> Self {
        Self {
            units: HashMap::new(),
            size: (1, 1),
            tab_size: config.tab_size,
            widths: vec![],
            accum_widths: vec![0],
        }
    }

    fn from_file(path: &str, config: Config) -> io::Result<Self> {
        let mut buf = String::new();
        File::options()
            .read(true)
            .write(true)
            .open(path)?
            .read_to_string(&mut buf)?;

        Ok(Self::from_str(&buf, config))
    }

    fn from_str(buf: &str, config: Config) -> Self {
        let mut contents: Vec<Vec<&str>> = vec![];

        let lines = buf.lines();
        for line in lines {
            contents.push(line.split('\t').collect());
        }

        let mut units_map = HashMap::new();
        let (widths, accum_widths) = get_widths(&contents, config.tab_size);

        let rows = contents.len();
        let cols = widths.len();

        let mut row: usize = 0;
        for line in contents {
            let mut col: usize = 0;
            let mut items = line.into_iter();
            while let Some(s) = items.next() {
                if !s.is_empty() {
                    units_map.insert((col, row), Unit::from(s));
                }

                let width = UnicodeWidthStr::width(s) / config.tab_size + 1;
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
            size: (cols, rows),
            tab_size: config.tab_size,
            widths,
            accum_widths,
        }
    }
}

/// Get column widths and accumulated widths from a 2-D vector.
fn get_widths(contents: &[Vec<&str>], tab_size: usize) -> (Vec<usize>, Vec<usize>) {
    let mut widths: Vec<usize> = vec![];

    for line in contents {
        let mut index: usize = 0;
        let mut items = line.iter().peekable();

        'outer: while let Some(&item) = items.next() {
            let mut width: usize = UnicodeWidthStr::width(item) / tab_size + 1;

            while let Some(&&following) = items.peek() {
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

    let mut accum_widths = vec![0];
    for i in 0..widths.len() {
        accum_widths.push(widths[i] + accum_widths[i]);
    }

    (widths, accum_widths)
}

/// Check if given `val` lies in `lbd..lbd + ofs`.
fn is_in_offset_bounds<T>(val: T, lbd: T, ofs: T) -> bool
where
    T: PartialOrd + Add<Output = T>,
{
    lbd <= val && val < lbd + ofs
}

fn read_line_initial_text(initial: &str) -> io::Result<String> {
    if initial.is_empty() {
        execute!(stdout(), Clear(ClearType::UntilNewLine))?;
    } else {
        execute!(
            stdout(),
            MoveLeft(initial.len() as u16),
            Clear(ClearType::UntilNewLine),
            Print(initial),
        )?;
    }

    let mut chars: Vec<char> = initial.chars().collect();

    loop {
        if let Event::Key(event) = event::read()? {
            match event {
                KeyEvent {
                    code: KeyCode::Backspace,
                    ..
                } => {
                    if chars.pop().is_some() {
                        execute!(stdout(), MoveLeft(1), Clear(ClearType::UntilNewLine))?;
                    }
                }
                KeyEvent {
                    code: KeyCode::Char(c),
                    ..
                } => {
                    chars.push(c);
                    let mut bytes_char = [0; 4];
                    c.encode_utf8(&mut bytes_char);

                    print!("{}", c.encode_utf8(&mut bytes_char));
                    stdout().flush()?;
                }
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    break;
                }
                _ => (),
            }
        }
    }

    Ok(chars.iter().collect())
}

#[derive(Debug)]
struct Unit {
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

#[derive(Debug, Default)]
enum Mode {
    #[default]
    Navigate,
    Edit,
    Command,
    Quit,
}
