use std::collections::HashMap;
use std::fs::File;
use std::io::{self, stdout, Read, Stdout, Write};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Print, ResetColor, SetAttribute};
use crossterm::{cursor, event, execute, style, terminal, ExecutableCommand};

use unicode_width::UnicodeWidthStr;

pub struct Session {
    term: Stdout,
    mode: Mode,
    file_path: Option<String>,
    sheet: Sheet,
    pos: (usize, usize),
    corner: (usize, usize),
}

impl Session {
    pub fn new(config: Config, args: &[String]) -> io::Result<Self> {
        let term = stdout();
        let mode = Mode::Navigate;
        let file_path = args.get(1).cloned();
        let sheet = match &file_path {
            Some(f) => Sheet::from_file(f, config)?,
            None => Sheet::new(config),
        };

        Ok(Self {
            term,
            mode,
            file_path,
            sheet,
            pos: (0, 0),
            corner: (0, 0),
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        execute!(self.term, terminal::EnterAlternateScreen)?;

        loop {
            match self.mode {
                Mode::Navigate => {
                    terminal::enable_raw_mode()?;
                    self.navigate()?;
                }
                Mode::Edit => {
                    terminal::disable_raw_mode()?;
                    self.modify()?;
                    self.refresh()?;
                }
                Mode::Command => {
                    terminal::disable_raw_mode()?;
                    self.command()?;
                    self.refresh()?;
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

        self.term.execute(cursor::MoveTo(
            ((self.sheet.accum_widths[self.pos.0] - self.sheet.accum_widths[self.corner.0])
                * self.sheet.tab_size)
                .try_into()
                .unwrap(),
            (self.pos.1 - self.corner.1).try_into().unwrap(),
        ))?;

        let buf = match self.sheet.units.get(&self.pos) {
            Some(unit) => unit.content.to_owned(),
            None => String::new(),
        };
        execute!(
            self.term,
            SetAttribute(style::Attribute::Reverse),
            Print(&buf),
            ResetColor,
        )?;

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
                } => {}

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

                _ => {
                    self.mode = Mode::Edit;
                }
            }
        }

        Ok(())
    }

    fn move_cursor_by(&mut self, x: isize, y: isize) -> io::Result<()> {
        let size = terminal::size()?;

        self.pos.0 = self.pos.0.saturating_add_signed(x);
        self.pos.1 = self.pos.1.saturating_add_signed(y);

        if !(self.corner.0 <= self.pos.0 && self.pos.0 < self.corner.0 + size.0 as usize - 1) {
            self.corner.0 = self.corner.0.saturating_add_signed(x);
        }
        if !(self.corner.1 <= self.pos.1 && self.pos.1 < self.corner.1 + size.1 as usize - 1) {
            self.corner.1 = self.corner.1.saturating_add_signed(y);
        }

        Ok(())
    }

    fn modify(&mut self) -> io::Result<()> {
        let mut buf = match self.sheet.units.get(&self.pos) {
            Some(unit) => unit.content.to_owned(),
            None => String::new(),
        };
        execute!(
            self.term,
            SetAttribute(style::Attribute::Reverse),
            Print(&buf),
        )?;
        io::stdin().read_line(&mut buf)?;
        execute!(self.term, ResetColor)?;

        self.sheet
            .units
            .entry(self.pos)
            .and_modify(|unit| {
                unit.content = buf.trim().to_owned();
            })
            .or_insert(Unit {
                content: buf.trim().to_owned(),
            });

        self.mode = Mode::Navigate;

        Ok(())
    }

    fn command(&mut self) -> io::Result<()> {
        self.term
            .execute(cursor::MoveTo(0, terminal::size().unwrap().1 - 1))?;
        print!(":");
        self.term.flush()?;

        let mut command = String::new();
        io::stdin().read_line(&mut command)?;

        self.parse_command(&command.trim())?;

        Ok(())
    }

    fn quit(&mut self) -> io::Result<()> {
        execute!(self.term, terminal::LeaveAlternateScreen)?;

        Ok(())
    }

    fn print(&mut self) -> io::Result<()> {
        for unit in &self.sheet.units {
            if !(self.corner.1 <= unit.0 .1
                && unit.0 .1 < self.corner.1 + terminal::size().unwrap().1 as usize - 1)
            {
                continue;
            }
            let col = (self.sheet.accum_widths[unit.0 .0.saturating_sub(self.corner.0)])
                * self.sheet.tab_size;
            let row = unit.0 .1.saturating_sub(self.corner.1);
            self.term.execute(cursor::MoveTo(
                col.try_into().unwrap(),
                row.try_into().unwrap(),
            ))?;
            print!("{:1}", unit.1.content);
        }

        Ok(())
    }

    fn refresh(&mut self) -> io::Result<()> {
        execute!(self.term, terminal::Clear(terminal::ClearType::All))?;

        self.print()?;

        Ok(())
    }

    fn parse_command(&mut self, command: &str) -> io::Result<()> {
        match command {
            "w" => {
                self.save()?;
            }
            "q" => {
                self.mode = Mode::Quit;
            }
            _ => {
                self.mode = Mode::Navigate;
            }
        }

        Ok(())
    }

    fn save(&self) -> io::Result<()> {
        let file_path = match &self.file_path {
            Some(fp) => fp.to_owned(),
            None => {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf)?;

                buf
            }
        };

        let mut file = File::options().create(true).write(true).open(file_path)?;

        for row in 0..self.sheet.rows {
            for col in 0..self.sheet.cols {
                if let Some(u) = self.sheet.units.get(&(col, row)) {
                    file.write_all(u.content.as_bytes())?;

                    let width = UnicodeWidthStr::width(u.content.as_str()) / self.sheet.tab_size + 1;
                    file.write_all(&b"\t".repeat(self.sheet.widths[col] - width + 1))?;
                };
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
    rows: usize,
    cols: usize,
    tab_size: usize,
    widths: Vec<usize>,
    accum_widths: Vec<usize>,
}

impl Sheet {
    fn new(config: Config) -> Self {
        Self {
            units: HashMap::new(),
            rows: 1,
            cols: 1,
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
                units_map.insert(
                    (col, row),
                    Unit {
                        content: String::from(s),
                    },
                );

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
            rows,
            cols,
            tab_size: config.tab_size,
            widths,
            accum_widths,
        }
    }
}

fn get_widths(contents: &[Vec<&str>], tab_size: usize) -> (Vec<usize>, Vec<usize>) {
    let mut widths: Vec<usize> = vec![];

    for line in contents {
        let mut index: usize = 0;
        let mut items = line.iter().peekable();

        'l: while let Some(item) = items.next() {
            let mut width: usize = UnicodeWidthStr::width(*item) / tab_size + 1;

            while let Some(following) = items.peek() {
                if (**following).is_empty() {
                    items.next();
                    width += 1;
                } else {
                    break;
                }
            }

            while let Some(prev_width) = widths.get(index) {
                if width.saturating_sub(*prev_width) > 0 {
                    if index == widths.len() - 1 {
                        widths[index] = width;
                    } else {
                        width -= prev_width;
                        index += 1;
                    }
                } else {
                    index += 1;
                    continue 'l;
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

#[derive(Debug)]
struct Unit {
    content: String,
}

enum Mode {
    Navigate,
    Edit,
    Command,
    Quit,
}
