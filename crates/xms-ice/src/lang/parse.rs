//! Tokens to a syntax tree: the snippet's header (its name, metadata and
//! parameters), its body, and the functions written before it.

use super::lex::{lex, Pos, Tok};
use super::Error;

/// A type as the text writes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Point,
    Vector,
    Normal,
    Color,
    Vector2,
    Vector4,
    Quat,
    Matrix,
    String,
    Geometry,
    Void,
}

impl Type {
    pub fn of(word: &str) -> Option<Type> {
        Some(match word {
            "int" => Type::Int,
            "float" => Type::Float,
            "point" => Type::Point,
            "vector" => Type::Vector,
            "normal" => Type::Normal,
            "color" => Type::Color,
            "vector2" => Type::Vector2,
            "vector4" => Type::Vector4,
            "quat" => Type::Quat,
            "matrix" => Type::Matrix,
            "string" => Type::String,
            "geometry" => Type::Geometry,
            "void" => Type::Void,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Type::Int => "int",
            Type::Float => "float",
            Type::Point => "point",
            Type::Vector => "vector",
            Type::Normal => "normal",
            Type::Color => "color",
            Type::Vector2 => "vector2",
            Type::Vector4 => "vector4",
            Type::Quat => "quat",
            Type::Matrix => "matrix",
            Type::String => "string",
            Type::Geometry => "geometry",
            Type::Void => "void",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub kind: ExprKind,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    Int(i32),
    Float(f32),
    Str(String),
    Var(String),
    /// `@name`.
    Attr(String),
    /// A function, a built-in, or a type's constructor (`point(0)`).
    Call(String, Vec<Expr>),
    /// `-`, `!`.
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// `p.x`, `c.r`.
    Member(Box<Expr>, String),
    /// `a[i]`, `p[0]`.
    Index(Box<Expr>, Box<Expr>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    Block(Vec<Stmt>),
    /// `float x = 1, y;`, `point pts[] = …;`
    Decl {
        ty: Type,
        array: bool,
        names: Vec<(String, Option<Expr>, Pos)>,
    },
    /// `vector @vel;`, `float @mask = 0;`
    AttrDecl {
        ty: Type,
        name: String,
        init: Option<Expr>,
        pos: Pos,
    },
    /// `x = …`, `x += …` (`op` is `=`, `+=`, `-=`, `*=` or `/=`), and `x++`.
    Assign {
        target: Expr,
        op: &'static str,
        value: Expr,
        pos: Pos,
    },
    Expr(Expr),
    If {
        cond: Expr,
        then: Box<Stmt>,
        otherwise: Option<Box<Stmt>>,
    },
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Expr>,
        step: Option<Box<Stmt>>,
        body: Box<Stmt>,
        pos: Pos,
    },
    While {
        cond: Expr,
        body: Box<Stmt>,
        pos: Pos,
    },
    Return(Option<Expr>, Pos),
}

/// A parameter of the snippet, or of a function.
#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub array: bool,
    pub output: bool,
    pub default: Option<Expr>,
    /// `[[ float min = 0, string label = "Amount" ]]`: name, value.
    pub meta: Vec<(String, Expr)>,
    pub pos: Pos,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    pub name: String,
    pub ty: Type,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub pos: Pos,
}

/// A snippet as written: its functions, its header and its body.
#[derive(Clone, Debug, PartialEq)]
pub struct Snippet {
    pub name: String,
    pub meta: Vec<(String, Expr)>,
    pub params: Vec<Param>,
    pub body: Vec<Stmt>,
    pub functions: Vec<Function>,
    pub pos: Pos,
}

impl Snippet {
    /// A metadata entry of the header, as a string (`context`).
    pub fn meta_str(&self, name: &str) -> Option<&str> {
        self.meta
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, e)| match &e.kind {
                ExprKind::Str(s) => Some(s.as_str()),
                _ => None,
            })
    }
}

struct Parser {
    toks: Vec<(Tok, Pos)>,
    at: usize,
}

pub fn parse(text: &str) -> Result<Snippet, Error> {
    let mut p = Parser {
        toks: lex(text)?,
        at: 0,
    };
    let mut functions = vec![];
    let mut snippet = None;
    while p.peek() != &Tok::End {
        let pos = p.pos();
        if p.word("snippet") {
            if snippet.is_some() {
                return Err(Error::at(pos, "One snippet per text"));
            }
            let name = p.ident()?;
            let meta = p.meta()?;
            let params = p.params(true)?;
            let body = p.block()?;
            snippet = Some(Snippet {
                name,
                meta,
                params,
                body,
                functions: vec![],
                pos,
            });
        } else {
            let ty = p.ty()?;
            let name = p.ident()?;
            let params = p.params(false)?;
            let body = p.block()?;
            functions.push(Function {
                name,
                ty,
                params,
                body,
                pos,
            });
        }
    }
    let mut s = snippet.ok_or_else(|| Error::at(p.pos(), "No snippet: write `snippet name (…) { … }`"))?;
    s.functions = functions;
    Ok(s)
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.at].0
    }
    fn peek_at(&self, k: usize) -> &Tok {
        &self.toks[(self.at + k).min(self.toks.len() - 1)].0
    }
    fn pos(&self) -> Pos {
        self.toks[self.at].1
    }
    fn next(&mut self) -> (Tok, Pos) {
        let t = self.toks[self.at].clone();
        if self.at + 1 < self.toks.len() {
            self.at += 1;
        }
        t
    }
    fn sym(&mut self, s: &str) -> bool {
        if matches!(self.peek(), Tok::Sym(x) if *x == s) {
            self.next();
            true
        } else {
            false
        }
    }
    fn is_sym(&self, k: usize, s: &str) -> bool {
        matches!(self.peek_at(k), Tok::Sym(x) if *x == s)
    }
    fn expect(&mut self, s: &str) -> Result<(), Error> {
        if self.sym(s) {
            Ok(())
        } else {
            Err(Error::at(
                self.pos(),
                format!("Expected `{s}`, found {}", shown(self.peek())),
            ))
        }
    }
    fn word(&mut self, w: &str) -> bool {
        if matches!(self.peek(), Tok::Ident(x) if x == w) {
            self.next();
            true
        } else {
            false
        }
    }
    fn ident(&mut self) -> Result<String, Error> {
        match self.next() {
            (Tok::Ident(name), _) if Type::of(&name).is_none() => Ok(name),
            (t, pos) => Err(Error::at(pos, format!("Expected a name, found {}", shown(&t)))),
        }
    }
    fn ty(&mut self) -> Result<Type, Error> {
        match self.next() {
            (Tok::Ident(w), pos) => Type::of(&w).ok_or_else(|| Error::at(pos, format!("`{w}` is not a type"))),
            (t, pos) => Err(Error::at(pos, format!("Expected a type, found {}", shown(&t)))),
        }
    }
    /// Whether a type starts here, not its constructor (`point(0)`).
    fn at_type(&self) -> bool {
        matches!(self.peek(), Tok::Ident(w) if Type::of(w).is_some()) && !self.is_sym(1, "(")
    }

    /// `[[ type name = value, … ]]`, or nothing.
    fn meta(&mut self) -> Result<Vec<(String, Expr)>, Error> {
        let mut out = vec![];
        if !(self.is_sym(0, "[") && self.is_sym(1, "[")) {
            return Ok(out);
        }
        self.next();
        self.next();
        loop {
            self.ty()?;
            let name = self.ident()?;
            self.expect("=")?;
            out.push((name, self.expr()?));
            if !self.sym(",") {
                break;
            }
        }
        self.expect("]")?;
        self.expect("]")?;
        Ok(out)
    }

    fn params(&mut self, snippet: bool) -> Result<Vec<Param>, Error> {
        self.expect("(")?;
        let mut out: Vec<Param> = vec![];
        while !self.sym(")") {
            if !out.is_empty() {
                self.expect(",")?;
            }
            let pos = self.pos();
            let output = self.word("output");
            let ty = self.ty()?;
            let name = self.ident()?;
            let array = self.sym("[") && {
                self.expect("]")?;
                true
            };
            let default = if self.sym("=") { Some(self.expr()?) } else { None };
            let meta = self.meta()?;
            if snippet && default.is_none() && !matches!(ty, Type::Geometry) && !array {
                return Err(Error::at(
                    pos,
                    format!("{name} needs a default value: `{} {name} = …`", ty.name()),
                ));
            }
            if out.iter().any(|p| p.name == name) {
                return Err(Error::at(pos, format!("Two parameters named {name}")));
            }
            out.push(Param {
                name,
                ty,
                array,
                output,
                default,
                meta,
                pos,
            });
        }
        Ok(out)
    }

    fn block(&mut self) -> Result<Vec<Stmt>, Error> {
        self.expect("{")?;
        let mut out = vec![];
        while !self.sym("}") {
            if self.peek() == &Tok::End {
                return Err(Error::at(self.pos(), "A block is never closed (`}`)"));
            }
            out.push(self.stmt()?);
        }
        Ok(out)
    }

    fn stmt(&mut self) -> Result<Stmt, Error> {
        let pos = self.pos();
        if self.is_sym(0, "{") {
            return Ok(Stmt::Block(self.block()?));
        }
        if self.word("if") {
            self.expect("(")?;
            let cond = self.expr()?;
            self.expect(")")?;
            let then = Box::new(self.stmt()?);
            let otherwise = if self.word("else") {
                Some(Box::new(self.stmt()?))
            } else {
                None
            };
            return Ok(Stmt::If { cond, then, otherwise });
        }
        if self.word("for") {
            self.expect("(")?;
            let init = if self.sym(";") {
                None
            } else {
                Some(Box::new(self.simple()?))
            };
            if init.is_some() {
                self.expect(";")?;
            }
            let cond = if self.is_sym(0, ";") { None } else { Some(self.expr()?) };
            self.expect(";")?;
            let step = if self.is_sym(0, ")") {
                None
            } else {
                Some(Box::new(self.simple()?))
            };
            self.expect(")")?;
            let body = Box::new(self.stmt()?);
            return Ok(Stmt::For {
                init,
                cond,
                step,
                body,
                pos,
            });
        }
        if self.word("while") {
            self.expect("(")?;
            let cond = self.expr()?;
            self.expect(")")?;
            let body = Box::new(self.stmt()?);
            return Ok(Stmt::While { cond, body, pos });
        }
        if self.word("return") {
            let value = if self.is_sym(0, ";") { None } else { Some(self.expr()?) };
            self.expect(";")?;
            return Ok(Stmt::Return(value, pos));
        }
        let s = self.simple()?;
        self.expect(";")?;
        Ok(s)
    }

    /// A declaration, an assignment or an expression: what a statement, a
    /// `for`'s start and its step are, without the `;`.
    fn simple(&mut self) -> Result<Stmt, Error> {
        let pos = self.pos();
        if self.at_type() {
            let ty = self.ty()?;
            if let Tok::Attr(name) = self.peek().clone() {
                self.next();
                let init = if self.sym("=") { Some(self.expr()?) } else { None };
                return Ok(Stmt::AttrDecl { ty, name, init, pos });
            }
            let mut names = vec![];
            let mut array = false;
            loop {
                let at = self.pos();
                let name = self.ident()?;
                if self.sym("[") {
                    self.expect("]")?;
                    array = true;
                }
                let init = if self.sym("=") { Some(self.expr()?) } else { None };
                names.push((name, init, at));
                if !self.sym(",") {
                    break;
                }
            }
            return Ok(Stmt::Decl { ty, array, names });
        }
        let target = self.expr()?;
        for op in ["=", "+=", "-=", "*=", "/="] {
            if self.sym(op) {
                let value = self.expr()?;
                return Ok(Stmt::Assign { target, op, value, pos });
            }
        }
        for (op, by) in [("++", "+="), ("--", "-=")] {
            if self.sym(op) {
                let value = Expr {
                    kind: ExprKind::Int(1),
                    pos,
                };
                return Ok(Stmt::Assign {
                    target,
                    op: by,
                    value,
                    pos,
                });
            }
        }
        Ok(Stmt::Expr(target))
    }

    fn expr(&mut self) -> Result<Expr, Error> {
        let cond = self.binary(0)?;
        if self.is_sym(0, "?") {
            let pos = self.pos();
            self.next();
            let a = self.expr()?;
            self.expect(":")?;
            let b = self.expr()?;
            return Ok(Expr {
                kind: ExprKind::Ternary(Box::new(cond), Box::new(a), Box::new(b)),
                pos,
            });
        }
        Ok(cond)
    }

    /// Binary operators by precedence, lowest first.
    fn binary(&mut self, level: usize) -> Result<Expr, Error> {
        const LEVELS: [&[&str]; 6] = [
            &["||"],
            &["&&"],
            &["==", "!="],
            &["<", "<=", ">", ">="],
            &["+", "-"],
            &["*", "/", "%"],
        ];
        if level == LEVELS.len() {
            return self.unary();
        }
        let mut left = self.binary(level + 1)?;
        loop {
            let pos = self.pos();
            let Some(op) = LEVELS[level].iter().find(|s| self.is_sym(0, s)) else {
                return Ok(left);
            };
            self.next();
            let right = self.binary(level + 1)?;
            left = Expr {
                kind: ExprKind::Binary(op, Box::new(left), Box::new(right)),
                pos,
            };
        }
    }

    fn unary(&mut self) -> Result<Expr, Error> {
        let pos = self.pos();
        for op in ["-", "!"] {
            if self.sym(op) {
                let e = self.unary()?;
                return Ok(Expr {
                    kind: ExprKind::Unary(op, Box::new(e)),
                    pos,
                });
            }
        }
        if self.sym("+") {
            return self.unary();
        }
        self.postfix()
    }

    fn postfix(&mut self) -> Result<Expr, Error> {
        let mut e = self.primary()?;
        loop {
            let pos = self.pos();
            // `[[` after a value is metadata (a parameter's), not an index.
            if self.is_sym(0, "[") && !self.is_sym(1, "[") {
                self.next();
                let i = self.expr()?;
                self.expect("]")?;
                e = Expr {
                    kind: ExprKind::Index(Box::new(e), Box::new(i)),
                    pos,
                };
            } else if self.sym(".") {
                let member = match self.next() {
                    (Tok::Ident(m), _) => m,
                    (t, at) => {
                        return Err(Error::at(
                            at,
                            format!("Expected x, y, z, w (or r, g, b), found {}", shown(&t)),
                        ))
                    }
                };
                e = Expr {
                    kind: ExprKind::Member(Box::new(e), member),
                    pos,
                };
            } else {
                return Ok(e);
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, Error> {
        let (tok, pos) = self.next();
        let kind = match tok {
            Tok::Int(i) => ExprKind::Int(i),
            Tok::Float(f) => ExprKind::Float(f),
            Tok::Str(s) => ExprKind::Str(s),
            Tok::Attr(name) => ExprKind::Attr(name),
            Tok::Ident(name) if self.is_sym(0, "(") => {
                self.next();
                let mut args = vec![];
                while !self.sym(")") {
                    if !args.is_empty() {
                        self.expect(",")?;
                    }
                    args.push(self.expr()?);
                }
                ExprKind::Call(name, args)
            }
            Tok::Ident(name) if Type::of(&name).is_some() => {
                return Err(Error::at(
                    pos,
                    format!("`{name}` is a type: a declaration starts a statement"),
                ))
            }
            Tok::Ident(name) => ExprKind::Var(name),
            Tok::Sym("(") => {
                let e = self.expr()?;
                self.expect(")")?;
                return Ok(e);
            }
            t => return Err(Error::at(pos, format!("Expected a value, found {}", shown(&t)))),
        };
        Ok(Expr { kind, pos })
    }
}

fn shown(t: &Tok) -> String {
    match t {
        Tok::Int(i) => i.to_string(),
        Tok::Float(f) => f.to_string(),
        Tok::Str(s) => format!("\"{s}\""),
        Tok::Ident(w) => format!("`{w}`"),
        Tok::Attr(a) => format!("`@{a}`"),
        Tok::Sym(s) => format!("`{s}`"),
        Tok::End => "the end".into(),
    }
}
