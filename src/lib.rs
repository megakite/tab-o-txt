use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};

use console::Term;
use console::{measure_text_width, style};

pub struct Session {
    config: Config,
    term: Term,
    mode: Mode,
    file_path: Option<String>,
    sheet: Sheet,
}

impl Session {
    pub fn new(config: Config, args: &[String]) -> io::Result<Self> {
        let term = Term::stdout();
        let mode = Mode::Navigate;
        let file_path = args.get(1).cloned();
        let sheet = match &file_path {
            Some(f) => Sheet::from(f.to_owned())?,
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
        self.term.clear_screen()?;
        self.print()?;

        loop {
            match self.mode {
                Mode::Navigate => self.navigate()?,
                Mode::Modify => self.modify()?,
                Mode::Command => self.command()?,

                Mode::Exit => break,
            }
        }

        Ok(())
    }

    fn navigate(&mut self) -> io::Result<()> {
        self.term.move_cursor_to(
            self.sheet.active_pos.1 * self.sheet.tab_size,
            self.sheet.active_pos.0,
        )?;

        match self.term.read_key()? {
            console::Key::ArrowDown     => self.sheet.active_pos.0 += 1,
            console::Key::ArrowRight    => self.sheet.active_pos.1 += 1,
            console::Key::ArrowUp       => self.sheet.active_pos.0 -= 1,
            console::Key::ArrowLeft     => self.sheet.active_pos.1 -= 1,

            console::Key::Char(';')     => self.mode = Mode::Command,
            console::Key::Escape        => self.mode = Mode::Exit,
            console::Key::Enter         => self.mode = Mode::Modify,

            _ => todo!(),
        }

        Ok(())
    }

    fn modify(&mut self) -> io::Result<()> {
        let buf = match self.sheet.units.get(&self.sheet.active_pos) {
            Some(unit) => unit.content.to_owned(),
            None => String::new(),
        };
        let new_buf = self.term.read_line_initial_text(&buf)?;

        self.sheet
            .units
            .entry(self.sheet.active_pos)
            .and_modify(|unit| {
                unit.content = new_buf.to_owned();
            })
            .or_insert(Unit {
                content: new_buf.to_owned(),
                width: new_buf.len() / self.sheet.tab_size,
            });

        self.mode = Mode::Navigate;

        Ok(())
    }

    fn command(&mut self) -> io::Result<()> {
        self.term
            .move_cursor_to(0, (self.term.size().0 - 1).into())?;

        let buf = String::from(";");
        let command = self.term.read_line_initial_text(&buf)?;

        self.parse_command(command)?;

        self.mode = Mode::Navigate;

        Ok(())
    }

    fn print(&self) -> io::Result<()> {
        for unit in &self.sheet.units {
            self.term
                .move_cursor_to(unit.0 .1 * self.sheet.tab_size, unit.0 .0)?;
            print!(
                "{:1$}",
                unit.1.content,
                measure_text_width(&unit.1.content) / self.sheet.tab_size
            );
        }

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
            for column in 0..self.sheet.columns {
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
    columns: usize,
    tab_size: usize,
    active_pos: (usize, usize),
}

impl Sheet {
    fn new() -> Self {
        Self {
            units: HashMap::new(),
            rows: 1,
            columns: 1,
            tab_size: 8,
            active_pos: (0, 0),
        }
    }

    fn from(path: String) -> io::Result<Self> {
        let mut file = File::options().read(true).write(true).open(path)?;
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;

        Ok(Self::parse(buf))
    }

    fn parse(buf: String) -> Self {
        let mut units = HashMap::new();
        let mut rows: usize = 1;
        let mut columns: usize = 1;

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
                match items.next() {
                    Some(item) => {
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
                    }
                    None => {
                        break count;
                    }
                };
                count += 1;
                column += 1;
            };
            row += 1;
            rows += 1;

            if columns < current_columns {
                columns = current_columns;
            }
        }

        Self {
            units,
            rows,
            columns,
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
    Modify,
    Command,
    Exit,
}
