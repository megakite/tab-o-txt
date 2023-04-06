use std::collections::HashMap;
use std::fs::File;
use std::io::{self, stdout, Read, Stdout, Write};

use crossterm::event::{Event, KeyCode, KeyEvent};
use crossterm::style::{Print, ResetColor, SetAttribute};
use crossterm::{cursor, event, execute, style, terminal, ExecutableCommand};

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
                width: buf.len() / self.sheet.tab_size + 1,
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
                (self.sheet.tab_size * unit.0 .1).try_into().unwrap(),
                unit.0 .0.try_into().unwrap(),
            ))?;
            print!(
                "{:1$}",
                &unit.1.content,
                &unit.1.width * self.sheet.tab_size,
            );
        }
        dbg!(&self.sheet.units);

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
    default_tab_size: usize,
    indent_type: IndentType,
}

impl Config {
    pub fn build(vars: &[(String, String)]) -> Result<Self, &'static str> {
        Ok(Self {
            default_tab_size: 8,
            indent_type: IndentType::Tab,
        })
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

        let mut lines = buf.lines();
        while let Some(ln) = lines.next() {
            contents.push(ln.split('\t').collect());
        }

        let orig_cols = contents.iter().map(|c| c.len()).max().unwrap();
        let orig_rows = contents.len();

        let mut units = HashMap::new();
        let widths: Vec<usize> = get_width_from_contents(contents, config.default_tab_size);

        units.insert(
            (0, 0),
            Unit {
                content: String::new(),
                width: 1,
            },
        );

        Self {
            units,
            rows: orig_rows,
            cols: orig_cols,
            tab_size: config.default_tab_size,
            active_pos: (0, 0),
        }
    }
}

fn get_width_from_contents(contents: Vec<Vec<&str>>, tab_size: usize) -> Vec<usize> {
    let mut widths: Vec<usize> = vec![];

    let mut lines = contents.iter();
    if let Some(first_line) = lines.next() {
        let mut iter = first_line.iter();
        let mut width_unit: usize = 0;

        while let Some(s) = iter.next() {
            if s.is_empty() {
                width_unit += 1;
            } else {
                if width_unit != 0 {
                    widths.push(width_unit);
                }
                width_unit = s.len() / tab_size + 1;
            }
        }
        widths.push(width_unit);
    }

    while let Some(line) = lines.next() {
        let mut width: usize = 0;
        let mut col: usize = 0;

        let mut items = line.iter();
        while let Some(s) = items.next() {
            if s.is_empty() {
                width += 1;
            } else {
                if width != 0 {
                    if let Some(n) = widths.get(col) {
                        if width > *n {
                            let diff = width - *n;
                            col += diff;
                        }
                    } else {
                        widths.push(width);
                    }
                    col += 1;
                }
                width = s.len() / tab_size + 1;
            }
        }

        if col == widths.len() - 1 && width > widths[col] {
            widths[col] = width;
        }
    
        if col > widths.len() - 1 {
            widths.push(width);
        }
    }

    return widths;
}

#[derive(Debug)]
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

enum IndentType {
    Tab,
    Space,
    Em,
}
