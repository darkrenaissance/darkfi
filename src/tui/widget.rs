use std::io::{Stdout, Write};

use termion::{cursor, raw::RawTerminal};

use crate::Result;

#[derive(Clone)]
pub struct Widget {
    pub width: usize,
    pub height: usize,
    pub x: usize,
    pub y: usize,
    title: String,
}

impl Widget {
    pub fn new(title: String) -> Result<Widget> {
        Ok(Widget { width: 0, height: 0, x: 0, y: 0, title })
    }

    pub fn print(
        &self,
        stdout: &mut RawTerminal<Stdout>,
        x: usize,
        y: usize,
        text: &str,
    ) -> Result<()> {
        write!(stdout, "{}{}", cursor::Goto(1 + x as u16, 1 + y as u16), text)?;
        Ok(())
    }

    pub fn print_border(&self, stdout: &mut RawTerminal<Stdout>) -> Result<()> {
        let x = self.x;
        let y = self.y;
        let width = self.width;
        let height = self.height;

        let hline = "─";
        let vline = "│";
        let topleftcorner = "┌";
        let toprightcorner = "┐";
        let downleftcorner = "└";
        let downrightcorner = "┘";

        self.print(stdout, x, y, topleftcorner)?;
        self.print(stdout, x, y + height, downleftcorner)?;

        self.print(stdout, x + width - 1, y, toprightcorner)?;
        self.print(stdout, x + width - 1, y + height, downrightcorner)?;

        for j in (y + 1)..(y + height) {
            self.print(stdout, x, j, vline)?;
            self.print(stdout, x + width - 1, j, vline)?;
        }

        for i in (x + 1)..(x + width - 1) {
            self.print(stdout, i, y, hline)?;
            self.print(stdout, i, y + height, hline)?;
        }

        Ok(())
    }

    pub fn clear_current_line(&self, stdout: &mut RawTerminal<Stdout>) -> Result<()> {
        let width = self.width;
        for i in self.x..(width - 3) {
            self.print(stdout, i, self.y, " ")?;
        }
        Ok(())
    }

    pub fn draw(&self, stdout: &mut RawTerminal<Stdout>) -> Result<()> {
        self.print_border(stdout)?;
        self.print(stdout, self.x + 3, self.y, &format!(" {} ", &self.title.clone()))?;
        Ok(())
    }
}

pub trait Layout {
    fn draw(
        &mut self,
        stdout: &mut RawTerminal<Stdout>,
        x: usize,
        y: usize,
        layout_width: usize,
        layout_height: usize,
    ) -> Result<(usize, usize)>;
}

pub struct VBox {
    widgets: Vec<Widget>,
    width: usize,
}

impl VBox {
    pub fn new(widgets: Vec<Widget>, width: usize) -> Self {
        Self { widgets, width }
    }
}

impl Layout for VBox {
    fn draw(
        &mut self,
        stdout: &mut RawTerminal<Stdout>,
        x: usize,
        y: usize,
        layout_width: usize,
        layout_height: usize,
    ) -> Result<(usize, usize)> {
        let len = self.widgets.len();

        let widget_width = layout_width / self.width;

        for (i, widget) in self.widgets.iter_mut().enumerate() {
            widget.width = widget_width - 1;
            widget.height = (layout_height / len) - 1;
            widget.x = x;
            widget.y = (widget.height + 1) * i + y;
            widget.draw(stdout)?;
        }

        Ok((widget_width, y))
    }
}

pub struct HBox {
    widgets: Vec<Widget>,
    pub height: usize,
}

impl HBox {
    pub fn new(widgets: Vec<Widget>, height: usize) -> Self {
        Self { widgets, height }
    }
}

impl Layout for HBox {
    fn draw(
        &mut self,
        stdout: &mut RawTerminal<Stdout>,
        x: usize,
        y: usize,
        layout_width: usize,
        layout_height: usize,
    ) -> Result<(usize, usize)> {
        let len = self.widgets.len();

        let widget_height = layout_height / self.height;

        for (i, widget) in self.widgets.iter_mut().enumerate() {
            widget.width = layout_width / len - 1;
            widget.height = widget_height - 1;
            widget.x = (widget.width + 1) * i + x;
            widget.y = y;
            widget.draw(stdout)?;
        }

        Ok((x, widget_height))
    }
}
