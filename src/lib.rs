use std::collections::HashMap;
use std::fs::File;
use std::io::{self, stdout, Read, Stdout, Write};

use crossterm::cursor::{MoveToNextLine, MoveToPreviousLine};
use crossterm::event::{Event, KeyCode, KeyEvent};
use crossterm::style::{Print, ResetColor, SetAttribute};
use crossterm::{cursor, event, execute, style, terminal, ExecutableCommand};

pub struct Session {
    config: Config,
    term: Stdout,
    mode: Mode,
    file_path: Option<String>,
    sheet: Sheet,
}

impl Session {
    pub fn new(config: Config, args: &[String]) -> io::Result<Self> {
        let term = stdout();
        let mode = Mode::Navigate;
        let file_path = args.get(1).cloned();
        let sheet = match &file_path {
            Some(f) => Sheet::from_file(f.to_owned())?,
            None => Sheet::new(),
        };

        Ok(Self {
            config,
            term,
            mode,
            file_path,
            sheet,
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        execute!(self.term, terminal::EnterAlternateScreen)?;

        self.refresh()?;

        loop {
            match self.mode {
                Mode::Navigate => self.navigate()?,
                Mode::Edit => {
                    self.modify()?;
                    self.refresh()?;
                }
                Mode::Command => {
                    self.command()?;
                    self.refresh()?;
                }
                Mode::Quit => {
                    self.quit()?;

                    break;
                }
            }
        }

        Ok(())
    }

    fn navigate(&mut self) -> io::Result<()> {
        terminal::enable_raw_mode()?;

        self.print()?;

        self.term.execute(cursor::MoveTo(
            (self.sheet.active_pos.1 * self.sheet.tab_size)
                .try_into()
                .unwrap(),
            self.sheet.active_pos.0.try_into().unwrap(),
        ))?;

        if let Event::Key(event) = event::read()? {
            match event {
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                } => self.sheet.move_checked(Direction::Down)?,
                KeyEvent {
                    code: KeyCode::Right,
                    ..
                } => self.sheet.move_checked(Direction::Right)?,
                KeyEvent {
                    code: KeyCode::Up, ..
                } => self.sheet.move_checked(Direction::Up)?,
                KeyEvent {
                    code: KeyCode::Left,
                    ..
                } => self.sheet.move_checked(Direction::Left)?,

                KeyEvent {
                    code: KeyCode::Char(';'),
                    ..
                } => self.mode = Mode::Command,
                KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => self.mode = Mode::Edit,
                KeyEvent {
                    code: KeyCode::Esc, ..
                } => self.mode = Mode::Quit,

                _ => todo!(),
            }
        }

        Ok(())
    }

    fn modify(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;

        execute!(
            self.term,
            MoveToNextLine(1),
            SetAttribute(style::Attribute::Reverse),
            Print(" ".repeat(terminal::size()?.0.into())),
            MoveToPreviousLine(1)
        )?;
        let mut new_buf = String::new();
        io::stdin().read_line(&mut new_buf)?;
        execute!(self.term, ResetColor)?;

        self.sheet
            .units
            .entry(self.sheet.active_pos)
            .and_modify(|unit| {
                unit.content = new_buf.trim().to_owned();
            })
            .or_insert(Unit {
                content: new_buf.trim().to_owned(),
                width: new_buf.trim().len() / self.sheet.tab_size,
            });

        self.mode = Mode::Navigate;

        Ok(())
    }

    fn command(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;

        self.term.execute(cursor::MoveTo(0, 0))?;

        let mut command = String::new();
        io::stdin().read_line(&mut command)?;

        self.parse_command(command)?;

        self.mode = Mode::Navigate;

        Ok(())
    }

    fn quit(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;

        execute!(self.term, terminal::LeaveAlternateScreen)?;

        Ok(())
    }

    fn print(&mut self) -> io::Result<()> {
        for unit in &self.sheet.units {
            self.term.execute(cursor::MoveTo(
                (unit.0 .1 * self.sheet.tab_size).try_into().unwrap(),
                unit.0 .0.try_into().unwrap(),
            ))?;
            print!(
                "{:1$}",
                &unit.1.content,
                &unit.1.content.len() / (self.sheet.tab_size + 1)
            );
        }

        Ok(())
    }

    fn refresh(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;

        execute!(self.term, terminal::Clear(terminal::ClearType::All))?;

        self.print()?;

        Ok(())
    }

    fn parse_command(&mut self, command: String) -> io::Result<()> {
        if command.is_empty() {
            return Ok(());
        }

        self.save()?;

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
            for column in 0..self.sheet.cols {
                file.write(match self.sheet.units.get(&(row, column)) {
                    Some(unit) => unit.content.as_bytes(),
                    None => b"",
                })?;
                file.write(b"\t")?;
            }
            file.write(b"\n")?;
        }

        Ok(())
    }
}
pub struct Config {
    tab_size: usize,
}

impl Config {
    pub fn build(vars: &[(String, String)]) -> Result<Self, &'static str> {
        Ok(Self { tab_size: 8 })
    }
}

struct Sheet {
    units: HashMap<(usize, usize), Unit>,
    rows: usize,
    cols: usize,
    tab_size: usize,
    active_pos: (usize, usize),
}

impl Sheet {
    fn new() -> Self {
        Self {
            units: HashMap::new(),
            rows: 1,
            cols: 1,
            tab_size: 8,
            active_pos: (0, 0),
        }
    }

    fn move_checked(&mut self, dir: Direction) -> io::Result<()> {
        match dir {
            Direction::Down => {
                if self.active_pos.0 != self.rows - 1 {
                    self.active_pos.0 += 1
                }
            }
            Direction::Right => {
                if self.active_pos.1 != self.cols - 1 {
                    self.active_pos.1 += 1
                }
            }
            Direction::Up => {
                if self.active_pos.0 != 0 {
                    self.active_pos.0 -= 1
                }
            }
            Direction::Left => {
                if self.active_pos.1 != 0 {
                    self.active_pos.1 -= 1
                }
            }
        }

        Ok(())
    }

    fn from_file(path: String) -> io::Result<Self> {
        let mut file = File::options().read(true).write(true).open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        Ok(Self::from_string(buf))
    }

    fn from_string(buf: String) -> Self {
        let mut units = HashMap::new();

        let mut rows: usize = 1;
        let mut cols: usize = 1;

        let mut row: usize = 0;
        let mut lines = buf.lines();
        loop {
            let mut column: usize = 0;
            let mut items = match lines.next() {
                Some(line) => line.split('\t'),
                None => {
                    break;
                }
            };

            let mut count: usize = 1;
            let current_columns = loop {
                if let Some(item) = items.next() {
                    if item.is_empty() {
                        column += 1;
                        continue;
                    } else {
                        units.insert(
                            (row, column),
                            Unit {
                                content: String::from(item),
                                width: item.len() / 8,
                            },
                        );
                    }
                } else {
                    break count;
                };
                count += 1;
                column += 1;
            };
            row += 1;
            rows += 1;

            if cols < current_columns {
                cols = current_columns;
            }
        }

        Self {
            units,
            rows,
            cols,
            tab_size: 8,
            active_pos: (0, 0),
        }
    }
}

struct Unit {
    content: String,
    width: usize,
}

enum Mode {
    Navigate,
    Edit,
    Command,
    Quit,
}

enum Direction {
    Down,
    Right,
    Up,
    Left,
}
