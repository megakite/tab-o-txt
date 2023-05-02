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
}

impl Session {
    pub fn new(config: Config, args: &[String]) -> io::Result<Self> {
        let term = stdout();
        let mode = Mode::Navigate;
        let file_path = args.get(1).cloned();
        let sheet = match &file_path {
            Some(f) => Sheet::from_file(f, config)?,
            None => Sheet::new(),
        };

        Ok(Self {
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
            (self.sheet.accum_widths[self.sheet.active_pos.1] * self.sheet.tab_size)
                .try_into()
                .unwrap(),
            self.sheet.active_pos.0.try_into().unwrap(),
        ))?;

        if let Event::Key(event) = event::read()? {
            match event {
                KeyEvent {
                    code: KeyCode::Up, ..
                }
                | KeyEvent {
                    code: KeyCode::Enter,
                    modifiers: KeyModifiers::SHIFT,
                    ..
                } => self.sheet.move_checked(Direction::Up)?,
                KeyEvent {
                    code: KeyCode::Left,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Tab,
                    modifiers: KeyModifiers::SHIFT,
                    ..
                } => self.sheet.move_checked(Direction::Left)?,
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => self.sheet.move_checked(Direction::Down)?,
                KeyEvent {
                    code: KeyCode::Right,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Tab, ..
                } => self.sheet.move_checked(Direction::Right)?,

                KeyEvent {
                    code: KeyCode::Char(':'),
                    ..
                } => self.mode = Mode::Command,
                KeyEvent {
                    code: KeyCode::Esc, ..
                } => self.mode = Mode::Quit,

                _ => self.mode = Mode::Edit,
            }
        }

        Ok(())
    }

    fn modify(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;

        let mut buf = match self.sheet.units.get(&self.sheet.active_pos) {
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
            .entry(self.sheet.active_pos)
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
        terminal::disable_raw_mode()?;

        self.term.execute(cursor::MoveTo(0, 0))?;

        let mut command = String::new();
        io::stdin().read_line(&mut command)?;

        self.parse_command(&command)?;

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
                (self.sheet.accum_widths[unit.0 .1] * self.sheet.tab_size)
                    .try_into()
                    .unwrap(),
                (unit.0 .0).try_into().unwrap(),
            ))?;
            print!("{:1}", unit.1.content);
        }

        Ok(())
    }

    fn refresh(&mut self) -> io::Result<()> {
        terminal::disable_raw_mode()?;

        execute!(self.term, terminal::Clear(terminal::ClearType::All))?;

        self.print()?;

        Ok(())
    }

    fn parse_command(&mut self, command: &str) -> io::Result<()> {
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
            for col in 0..self.sheet.cols {
                if let Some(u) = self.sheet.units.get(&(row, col)) {
                    file.write_all(u.content.as_bytes())?;

                    let width = u.content.len() / self.sheet.tab_size + 1;
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

struct Sheet {
    units: HashMap<(usize, usize), Unit>,
    rows: usize,
    cols: usize,
    tab_size: usize,
    active_pos: (usize, usize),
    widths: Vec<usize>,
    accum_widths: Vec<usize>,
}

impl Sheet {
    fn new() -> Self {
        Self {
            units: HashMap::new(),
            rows: 1,
            cols: 1,
            tab_size: 8,
            active_pos: (0, 0),
            widths: vec![],
            accum_widths: vec![0],
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
        let (widths, accum_widths) = get_col_widths_from_contents(&contents, config.tab_size);

        let rows = contents.len();
        let cols = widths.len();

        let mut row: usize = 0;
        for line in contents {
            let mut col: usize = 0;
            let mut items = line.into_iter();
            while let Some(s) = items.next() {
                units_map.insert(
                    (row, col),
                    Unit {
                        content: String::from(s),
                    },
                );

                let width = UnicodeWidthStr::width(s) / config.tab_size + 1;
                let diff = widths[col].checked_sub(width).unwrap_or_default();
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
            active_pos: (0, 0),
            widths,
            accum_widths,
        }
    }
}

fn get_col_widths_from_contents(
    contents: &[Vec<&str>],
    tab_size: usize,
) -> (Vec<usize>, Vec<usize>) {
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
                if width.checked_sub(*prev_width).unwrap_or_default() > 0 {
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

enum Direction {
    Down,
    Right,
    Up,
    Left,
}
