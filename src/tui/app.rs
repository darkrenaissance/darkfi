use async_std::sync::Mutex;
use std::io::{Stdin, Stdout, Write};

use termion::{
    clear, cursor,
    event::Key,
    input::{Keys, TermRead},
    raw::{IntoRawMode, RawTerminal},
};

use crate::Result;
use super::Layout;

#[allow(dead_code)]
pub struct App {
    layouts: Vec<Box<dyn Layout>>,
    stdin: Mutex<Keys<Stdin>>,
    stdout: RawTerminal<Stdout>,
}

impl App {
    pub fn new() -> Result<Self> {
        let stdin = Mutex::new(std::io::stdin().keys());

        let stdout = std::io::stdout();
        let stdout = stdout.into_raw_mode()?;

        Ok(Self {
            stdin,
            stdout,
            layouts: vec![],
        })
    }

    fn clear(&mut self) -> Result<()> {
        self.hide_cursor()?;
        write!(self.stdout, "{}", clear::All)?;
        Ok(())
    }

    fn hide_cursor(&mut self) -> Result<()> {
        write!(self.stdout, "{}", cursor::Hide)?;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<()> {
        write!(self.stdout, "{}", cursor::Show)?;
        Ok(())
    }

    fn _move_the_cursor(&mut self, x: usize, y: usize) -> Result<()> {
        write!(self.stdout, "{}", cursor::Goto(1 + x as u16, 1 + y as u16))?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.stdout.flush()?;
        Ok(())
    }

    async fn _get_stdin_key(&self) -> Option<Key> {
        match self.stdin.lock().await.next() {
            Some(Ok(key)) => Some(key),
            _ => None,
        }
    }

    pub fn add_layout(&mut self, layout: Box<dyn Layout>) -> Result<()> {
        self.layouts.push(layout);
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        self.clear()?;

        let mut terminal_width = termion::terminal_size()?.0 as usize;
        let mut terminal_height = termion::terminal_size()?.1 as usize;

        let mut last_box_x = 0;
        let mut last_box_y = 0;

        for layout in self.layouts.iter_mut() {
            let (box_x, box_y) = layout.draw(
                &mut self.stdout,
                last_box_x,
                last_box_y,
                terminal_width as usize,
                terminal_height as usize,
            )?;

            if last_box_x != box_x {
                last_box_x += box_x;
                if terminal_width > box_x {
                    terminal_width -= box_x;
                } else {
                    break;
                }
            }

            if last_box_y != box_y {
                last_box_y += box_y;
                if terminal_height > box_y {
                    terminal_height -= box_y;
                } else {
                    break;
                }
            }
        }

        self.flush()?;

        async_std::task::sleep(std::time::Duration::from_secs(5)).await;

        self.show_cursor()?;

        Ok(())
    }
}
