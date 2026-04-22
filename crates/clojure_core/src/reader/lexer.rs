//! Character classifiers for the reader.

pub fn is_whitespace(c: char) -> bool {
    // Clojure treats commas as whitespace too.
    c.is_whitespace() || c == ','
}

pub fn is_digit(c: char) -> bool {
    c.is_ascii_digit()
}

pub fn is_macro_terminating(c: char) -> bool {
    // Delimiters + macro chars that end a token.
    matches!(c, '"' | ';' | '@' | '^' | '`' | '~' | '(' | ')' | '[' | ']' | '{' | '}' | '\\')
}

pub fn is_token_terminating(c: char) -> bool {
    is_whitespace(c) || is_macro_terminating(c)
}
