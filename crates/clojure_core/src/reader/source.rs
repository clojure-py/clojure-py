//! Source — wraps an input &str, tracks line/column, supports peek/advance/unread.

pub struct Source<'a> {
    input: &'a str,
    pos: usize,           // byte offset
    line: u32,
    column: u32,
    pushback: Option<char>,  // one char of unread support
}

impl<'a> Source<'a> {
    pub fn new(input: &'a str) -> Self {
        Self { input, pos: 0, line: 1, column: 1, pushback: None }
    }

    pub fn line(&self) -> u32 { self.line }
    pub fn column(&self) -> u32 { self.column }
    pub fn at_eof(&self) -> bool { self.pushback.is_none() && self.pos >= self.input.len() }

    /// Peek the next char without consuming.
    pub fn peek(&self) -> Option<char> {
        if let Some(c) = self.pushback { return Some(c); }
        self.input[self.pos..].chars().next()
    }

    /// Consume and return the next char.
    pub fn advance(&mut self) -> Option<char> {
        if let Some(c) = self.pushback.take() {
            self.track_pos(c);
            return Some(c);
        }
        let c = self.input[self.pos..].chars().next()?;
        self.pos += c.len_utf8();
        self.track_pos(c);
        Some(c)
    }

    /// Push a char back (one-char lookahead).
    pub fn unread(&mut self, c: char) {
        // Only one-char pushback is supported; a second unread overwrites.
        self.pushback = Some(c);
        // Note: we don't un-track line/col here. Line/col is an approximation
        // immediately after an unread — callers should capture line/col BEFORE
        // consuming a token, not after pushback.
    }

    fn track_pos(&mut self, c: char) {
        if c == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
    }
}
