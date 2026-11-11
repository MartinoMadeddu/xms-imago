//! The snippet's text as tokens, each with its line and column.

use super::Error;

/// A place in the snippet's text: line and column, from 1.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(i32),
    Float(f32),
    Str(String),
    Ident(String),
    /// `@name`, `@intrinsic:name`: an attribute, its name without the `@`.
    Attr(String),
    /// Punctuation and operators: `(`, `+=`, `[[`, `&&`…
    Sym(&'static str),
    End,
}

/// Longest first, so `+=` is not read as `+` and `=`. (`[[` and `]]` are two
/// brackets each: the parser reads metadata, so `a[b[0]]` stays an index.)
const SYMS: [&str; 32] = [
    "&&", "||", "==", "!=", "<=", ">=", "+=", "-=", "*=", "/=", "++", "--", "(", ")", "{", "}", "[", "]", ",", ";",
    ".", "?", ":", "+", "-", "*", "/", "%", "<", ">", "!", "=",
];

pub fn lex(text: &str) -> Result<Vec<(Tok, Pos)>, Error> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let (mut i, mut line, mut col) = (0usize, 1u32, 1u32);
    let at = |line, col| Pos { line, col };
    while i < chars.len() {
        let c = chars[i];
        let here = at(line, col);
        let step = |n: usize, i: &mut usize, col: &mut u32| {
            *i += n;
            *col += n as u32;
        };
        if c == '\n' {
            i += 1;
            line += 1;
            col = 1;
        } else if c.is_whitespace() {
            step(1, &mut i, &mut col);
        } else if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if c == '/' && chars.get(i + 1) == Some(&'*') {
            step(2, &mut i, &mut col);
            loop {
                match chars.get(i) {
                    None => return Err(Error::at(here, "A comment opened here is never closed (*/)")),
                    Some('*') if chars.get(i + 1) == Some(&'/') => {
                        step(2, &mut i, &mut col);
                        break;
                    }
                    Some('\n') => {
                        i += 1;
                        line += 1;
                        col = 1;
                    }
                    Some(_) => step(1, &mut i, &mut col),
                }
            }
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '.') {
                // An exponent's sign: 1e-3.
                let sign = matches!(chars[i], 'e' | 'E') && matches!(chars.get(i + 1), Some('+' | '-'));
                i += if sign { 2 } else { 1 };
            }
            let word: String = chars[start..i].iter().collect();
            col += (i - start) as u32;
            let float = word.contains(['.', 'e', 'E']) || word.ends_with('f');
            let tok = if float {
                word.trim_end_matches('f').parse().map(Tok::Float).ok()
            } else {
                word.parse().map(Tok::Int).ok()
            };
            out.push((
                tok.ok_or_else(|| Error::at(here, format!("Not a number: {word}")))?,
                here,
            ));
        } else if c.is_alphabetic() || c == '_' || c == '@' {
            let start = i;
            i += 1;
            // An attribute's name may hold a namespace: `@intrinsic:area`.
            while i < chars.len()
                && (chars[i].is_alphanumeric()
                    || chars[i] == '_'
                    || (c == '@' && chars[i] == ':' && chars.get(i + 1).is_some_and(|n| n.is_alphabetic())))
            {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            col += (i - start) as u32;
            out.push(match word.strip_prefix('@') {
                Some("") => return Err(Error::at(here, "An attribute's name after @")),
                Some(name) => (Tok::Attr(name.to_string()), here),
                None => (Tok::Ident(word), here),
            });
        } else if c == '"' {
            let mut s = String::new();
            step(1, &mut i, &mut col);
            loop {
                match chars.get(i) {
                    None | Some('\n') => return Err(Error::at(here, "A string opened here is never closed (\")")),
                    Some('"') => {
                        step(1, &mut i, &mut col);
                        break;
                    }
                    Some('\\') if i + 1 < chars.len() => {
                        s.push(match chars[i + 1] {
                            'n' => '\n',
                            't' => '\t',
                            other => other,
                        });
                        step(2, &mut i, &mut col);
                    }
                    Some(ch) => {
                        s.push(*ch);
                        step(1, &mut i, &mut col);
                    }
                }
            }
            out.push((Tok::Str(s), here));
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let Some(sym) = SYMS.iter().find(|s| rest.starts_with(**s)) else {
                return Err(Error::at(here, format!("Unexpected character {c:?}")));
            };
            step(sym.len(), &mut i, &mut col);
            out.push((Tok::Sym(sym), here));
        }
    }
    out.push((Tok::End, at(line, col)));
    Ok(out)
}
