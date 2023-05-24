use std::fs::File;
use std::io::{self, stdin, stdout, Read, Write};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{Attribute, Print, ResetColor, SetAttribute};
use crossterm::terminal::{Clear, ClearType};
use crossterm::{cursor, event, execute, terminal};

use sheet::Sheet;
use unicode_width::UnicodeWidthStr;

use crate::sheet;
use crate::util::{is_in_offset_bounds, read_line_initial_text};

pub struct Editor {
    mode: Mode,
    file_path: Option<String>,
    sheet: Sheet,
    /// Current cursor position. Zero-indexed. Represented in `(col, row)`.
    pos: (usize, usize),
    /// From where the table starts to be drawn. Zero-indexed. Represented in `(col, row)`.
    corner: (usize, usize),
}

impl Editor {
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
                ((self.sheet.accum_width_at(self.pos.0).unwrap()
                    - self.sheet.accum_width_at(self.corner.0).unwrap())
                    * self.sheet.tab_size()) as u16,
                (self.pos.1 - self.corner.1) as u16,
            )
        )?;

        if let Some(s) = self.sheet.content_at(self.pos) {
            execute!(
                stdout(),
                SetAttribute(Attribute::Reverse),
                Print(s),
                ResetColor,
            )?;
        }

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
                    self.move_pos_by(0, -1)?;
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
                    self.move_pos_by(-1, 0)?;
                }
                KeyEvent {
                    code: KeyCode::Down,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Enter,
                    ..
                } => {
                    self.move_pos_by(0, 1)?;
                }
                KeyEvent {
                    code: KeyCode::Right,
                    ..
                }
                | KeyEvent {
                    code: KeyCode::Tab, ..
                } => {
                    self.move_pos_by(1, 0)?;
                }
                KeyEvent {
                    code: KeyCode::PageDown,
                    ..
                } => {
                    self.move_pos_by(0, (terminal::size().unwrap().1 - 1) as isize)?;
                }
                KeyEvent {
                    code: KeyCode::PageUp,
                    ..
                } => {
                    self.move_pos_by(0, -((terminal::size().unwrap().1 - 1) as isize))?;
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

    fn move_pos_by(&mut self, x: isize, y: isize) -> io::Result<()> {
        let size = terminal::size()?;

        self.pos.0 = self
            .pos
            .0
            .saturating_add_signed(x)
            .clamp(0, self.sheet.size().0);
        self.pos.1 = self
            .pos
            .1
            .saturating_add_signed(y)
            .clamp(0, self.sheet.size().1);

        if !is_in_offset_bounds(
            *self.sheet.accum_width_at(self.pos.0).unwrap(),
            *self.sheet.accum_width_at(self.pos.0).unwrap(),
            (size.0 as usize - 1) / self.sheet.tab_size(),
        ) {
            self.corner.0 = self.corner.0.saturating_add_signed(x);
        }
        if !is_in_offset_bounds(self.pos.1, self.corner.1, size.1 as usize - 1) {
            self.corner.1 = self.corner.1.saturating_add_signed(y);
        }

        Ok(())
    }

    fn edit(&mut self) -> io::Result<()> {
        let mut buf = match self.sheet.content_at(self.pos) {
            Some(s) => s.to_owned(),
            None => String::new(),
        };
        buf = read_line_initial_text(&buf)?;

        self.sheet.edit(self.pos, &buf);

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

    fn quit(&self) -> io::Result<()> {
        execute!(stdout(), terminal::LeaveAlternateScreen)?;

        Ok(())
    }

    fn print(&self) -> io::Result<()> {
        let size: (u16, u16) = terminal::size()?;

        let cols = self.corner.0..self.corner.0 + (size.0 as usize - 1) / self.sheet.tab_size();
        for col in cols {
            for row in self.corner.1..self.corner.1 + size.1 as usize - 1 {
                if let Some(s) = self.sheet.content_at((col, row)) {
                    let (display_col, display_row) =
                        self.sheet.get_display_pos((col, row), self.corner);

                    execute!(
                        stdout(),
                        cursor::MoveTo(display_col as u16, display_row as u16),
                        Print(s),
                    )?;
                }
            }
        }

        Ok(())
    }

    fn refresh(&self) -> io::Result<()> {
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
        let mut file = File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(file_path)?;

        for row in 0..self.sheet.size().1 {
            let mut count: usize = 0;
            for col in 0..self.sheet.size().0 {
                if let Some(s) = self.sheet.content_at((col, row)) {
                    file.write_all(&b"\t".repeat(count))?;
                    file.write_all(s.as_bytes())?;

                    let width = UnicodeWidthStr::width(s) / self.sheet.tab_size() + 1;
                    count = 1 + (self.sheet.width_at(col).unwrap() - width);
                } else {
                    count += self.sheet.width_at(col).unwrap();
                }
            }
            file.write_all(b"\n")?;
        }

        Ok(())
    }
}

pub struct Config {
    pub tab_size: usize,
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

#[derive(Debug, Default)]
enum Mode {
    #[default]
    Navigate,
    Edit,
    Command,
    Quit,
}
