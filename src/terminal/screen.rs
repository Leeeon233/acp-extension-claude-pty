pub struct TerminalScreen {
    rows: u16,
    cols: u16,
    parser: vt100::Parser,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenSnapshot {
    pub rows: u16,
    pub cols: u16,
    pub text: String,
}

impl TerminalScreen {
    pub fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            parser: vt100::Parser::new(rows, cols, 0),
        }
    }

    pub fn process(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
    }

    pub fn text(&self) -> String {
        self.parser.screen().contents()
    }

    pub fn snapshot(&self) -> ScreenSnapshot {
        ScreenSnapshot {
            rows: self.rows,
            cols: self.cols,
            text: self.text(),
        }
    }
}
