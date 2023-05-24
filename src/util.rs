use std::{
    io::{self, stdout, Write},
    ops::Add,
};

use crossterm::{
    cursor::MoveLeft,
    event::{self, Event, KeyCode, KeyEvent},
    execute,
    style::Print,
    terminal::{Clear, ClearType},
};

/// Check if given `val` lies in `lbd..lbd + ofs`.
pub fn is_in_offset_bounds<T>(val: T, lbd: T, ofs: T) -> bool
where
    T: Ord + Add<Output = T>,
{
    lbd <= val && val < lbd + ofs
}

pub fn read_line_initial_text(initial: &str) -> io::Result<String> {
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
