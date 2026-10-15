//! Tokens, literals and trivia (`language.md` §2).

use crate::ast::Lit;
use crate::Diag;

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    Lit(Lit),
    Label(String),
    Capture(String),
    Punct(&'static str),
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
    /// Comments on their own lines before this token; an empty entry is a blank line.
    pub comments: Vec<String>,
    /// A blank line before this token (or before its first comment).
    pub blank_before: bool,
    /// A comment after this token on the same line.
    pub trailing: Option<String>,
}

const PUNCT: &[&str] = &[
    "->", "<-", "|>", "==", "!=", "<=", ">=", "{", "}", "(", ")", "[", "]", ";", ":", ",", ".", "=", "<", ">",
    "+", "-", "*", "/",
];

pub fn lex(src: &str) -> Result<Vec<Token>, Diag> {
    let mut lx = Lexer { s: src.as_bytes(), i: 0, line: 1, col: 1, start: (1, 1) };
    let mut out: Vec<Token> = Vec::new();
    loop {
        // Trivia: whitespace and comments. `gap` counts newlines since the last token or
        // comment; a gap of two or more newlines is a blank line.
        let mut comments = Vec::new();
        let mut blank = false;
        let mut gap = 0;
        while let Some(c) = lx.peek() {
            if c == b'\n' {
                gap += 1;
                lx.bump();
            } else if c == b' ' || c == b'\r' {
                lx.bump();
            } else if c == b'\t' {
                return Err(lx.diag("E0102", "tabs are not allowed; indent with 4 spaces"));
            } else if lx.starts_with("//") {
                let start = lx.i;
                while lx.peek().is_some_and(|c| c != b'\n') {
                    lx.bump();
                }
                let text = String::from_utf8_lossy(&lx.s[start..lx.i]).trim_end().to_string();
                if gap == 0 && comments.is_empty() && !out.is_empty() {
                    out.last_mut().unwrap().trailing = Some(text);
                } else {
                    // A blank line before the first comment marks the node; one between
                    // comments, or after the last, is kept as an empty entry.
                    if comments.is_empty() {
                        blank = gap >= 2;
                    } else if gap >= 2 {
                        comments.push(String::new());
                    }
                    comments.push(text);
                }
                gap = 0;
            } else {
                break;
            }
        }
        if comments.is_empty() {
            blank = gap >= 2;
        } else if gap >= 2 {
            comments.push(String::new());
        }
        let (line, col) = (lx.line, lx.col);
        let tok = match lx.peek() {
            None => Tok::Eof,
            Some(_) => lx.token()?,
        };
        let eof = tok == Tok::Eof;
        let blank_before = blank && !out.is_empty();
        out.push(Token { tok, line, col, comments, blank_before, trailing: None });
        if eof {
            return Ok(out);
        }
    }
}

struct Lexer<'a> {
    s: &'a [u8],
    i: usize,
    line: usize,
    col: usize,
    /// Where the current token starts, for diagnostics about literals.
    start: (usize, usize),
}

impl Lexer<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn at(&self, k: usize) -> Option<u8> {
        self.s.get(self.i + k).copied()
    }

    fn starts_with(&self, p: &str) -> bool {
        self.s[self.i..].starts_with(p.as_bytes())
    }

    fn bump(&mut self) {
        if self.s[self.i] == b'\n' {
            self.line += 1;
            self.col = 1;
        } else if self.s[self.i] & 0xC0 != 0x80 {
            self.col += 1;
        }
        self.i += 1;
    }

    fn diag(&self, code: &'static str, message: &str) -> Diag {
        Diag { code, line: self.line, col: self.col, message: message.to_string() }
    }

    /// A diagnostic at the start of the current token.
    fn diag_token(&self, code: &'static str, message: &str) -> Diag {
        Diag { code, line: self.start.0, col: self.start.1, message: message.to_string() }
    }

    fn ident(&mut self) -> String {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_') {
            self.bump();
        }
        String::from_utf8_lossy(&self.s[start..self.i]).into_owned()
    }

    fn token(&mut self) -> Result<Tok, Diag> {
        self.start = (self.line, self.col);
        let c = self.peek().unwrap();
        if c.is_ascii_alphabetic() || c == b'_' {
            return Ok(Tok::Ident(self.ident()));
        }
        if c == b'@' || c == b'$' {
            self.bump();
            if !self.peek().is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
                return Err(self.diag("E0103", "expected a name after `@` or `$`"));
            }
            let name = self.ident();
            return Ok(if c == b'@' { Tok::Label(name) } else { Tok::Capture(name) });
        }
        if c.is_ascii_digit() {
            return self.number_or_range(false);
        }
        // A negative bound starts a range: `-180.0..180.0`.
        if c == b'-' && self.at(1).is_some_and(|d| d.is_ascii_digit()) {
            let save = (self.i, self.line, self.col);
            self.bump();
            let lit = self.literal(true)?;
            if self.starts_with("..") {
                return self.range_from(lit);
            }
            (self.i, self.line, self.col) = save;
        }
        for p in PUNCT {
            if self.starts_with(p) {
                for _ in 0..p.len() {
                    self.bump();
                }
                return Ok(Tok::Punct(p));
            }
        }
        Err(self.diag("E0104", &format!("unexpected character `{}`", c as char)))
    }

    fn number_or_range(&mut self, negative: bool) -> Result<Tok, Diag> {
        let lit = self.literal(negative)?;
        if self.starts_with("..") {
            return self.range_from(lit);
        }
        Ok(Tok::Lit(lit))
    }

    fn range_from(&mut self, lo: Lit) -> Result<Tok, Diag> {
        self.bump();
        self.bump();
        let negative = self.peek() == Some(b'-');
        if negative {
            self.bump();
        }
        if !self.peek().is_some_and(|c| c.is_ascii_digit()) {
            return Err(self.diag_token("E0105", "a range needs a number after `..`"));
        }
        let hi = self.literal(negative)?;
        Ok(Tok::Lit(Lit::Range(Box::new(lo), Box::new(hi))))
    }

    fn digits(&mut self) -> String {
        let mut d = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                d.push(c as char);
            } else if c != b'_' {
                break;
            }
            self.bump();
        }
        d
    }

    /// An integer, float, amount or rate, with an optional minus already consumed.
    fn literal(&mut self, negative: bool) -> Result<Lit, Diag> {
        let int = self.digits();
        let mut frac = None;
        let mut exp = None;
        if self.peek() == Some(b'.') && self.at(1).is_some_and(|c| c.is_ascii_digit()) {
            self.bump();
            frac = Some(self.digits());
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            let sign = matches!(self.at(1), Some(b'+' | b'-'));
            let digit_at = if sign { 2 } else { 1 };
            if self.at(digit_at).is_some_and(|c| c.is_ascii_digit()) {
                self.bump();
                let mut e = String::new();
                if sign {
                    e.push(self.peek().unwrap() as char);
                    self.bump();
                }
                e.push_str(&self.digits());
                exp = Some(e);
            }
        }
        let suffix_ends = |k: usize| !self.at(k).is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
        let sign = if negative { "-" } else { "" };
        if self.peek() == Some(b'u') && suffix_ends(1) {
            self.bump();
            return self.amount(negative, &int, frac.as_deref(), exp.is_some(), 1_000);
        }
        if self.starts_with("mu") && suffix_ends(2) {
            self.bump();
            self.bump();
            return self.amount(negative, &int, frac.as_deref(), exp.is_some(), 1);
        }
        if self.starts_with("bp") && suffix_ends(2) {
            self.bump();
            self.bump();
            if negative || frac.is_some() || exp.is_some() {
                return Err(self.diag_token("E0106", "a rate is a whole, non-negative number of basis points"));
            }
            return Ok(Lit::Rate(int.parse().map_err(|_| self.diag_token("E0107", "rate too large"))?));
        }
        if frac.is_some() || exp.is_some() {
            let mut s = format!("{sign}{int}");
            if let Some(f) = frac {
                s.push('.');
                s.push_str(&f);
            }
            if let Some(e) = exp {
                s.push('e');
                s.push_str(&e);
            }
            return Ok(Lit::Float(s));
        }
        let v: i64 = int.parse().map_err(|_| self.diag_token("E0107", "integer too large"))?;
        Ok(Lit::Int(if negative { -v } else { v }))
    }

    fn amount(&self, negative: bool, int: &str, frac: Option<&str>, exp: bool, scale: u64) -> Result<Lit, Diag> {
        if negative || exp {
            return Err(self.diag_token("E0203", "amounts are written as plain non-negative numbers"));
        }
        let whole: u64 = int.parse().map_err(|_| self.diag_token("E0107", "amount too large"))?;
        let mut mu = whole * scale;
        if let Some(f) = frac {
            // Exact decimal: the fraction must be a whole number of µ.
            let digits = if scale == 1_000 { 3 } else { 0 };
            let (keep, rest) = f.split_at(f.len().min(digits));
            if rest.chars().any(|c| c != '0') {
                return Err(self.diag_token("E0101", "an amount must be a whole number of µ"));
            }
            let padded = format!("{keep:0<width$}", width = digits);
            if !padded.is_empty() {
                mu += padded.parse::<u64>().unwrap();
            }
        }
        Ok(Lit::Amount(mu))
    }
}
