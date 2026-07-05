//! Minimal Molang expression evaluator.
//!
//! Supports the tiny subset needed for vanilla-style idle/walk animations:
//! numeric literals, `+ - * /`, unary minus, parentheses, `math.sin`/`math.cos`
//! (which take DEGREES, per Molang semantics), `math.mod`, `math.abs`, and the
//! queries `query.anim_time` / `query.life_time` (and their `q.`/`t.`/`v.`
//! aliases resolve to the same time value or 0). Anything unrecognized
//! evaluates to `0.0` so a malformed expression degrades gracefully instead of
//! breaking rendering.

use std::fmt;

/// A parsed Molang expression, ready to evaluate against a [`Context`].
#[derive(Debug, Clone)]
pub enum Expr {
    Const(f32),
    AnimTime,
    LifeTime,
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Sin(Box<Expr>),
    Cos(Box<Expr>),
    Abs(Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
}

/// Runtime values fed to [`Expr::eval`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Context {
    /// Seconds elapsed within the current animation (`query.anim_time`).
    pub anim_time: f32,
    /// Seconds elapsed since the entity spawned (`query.life_time`).
    pub life_time: f32,
}

impl Expr {
    /// Parse a Molang string; unparseable input yields `Const(0.0)`.
    pub fn parse(src: &str) -> Self {
        Parser::new(src).parse().unwrap_or(Expr::Const(0.0))
    }

    /// Convenience: parse a JSON value that may be a number or a Molang string.
    pub fn from_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Number(n) => Expr::Const(n.as_f64().unwrap_or(0.0) as f32),
            serde_json::Value::String(s) => Expr::parse(s),
            _ => Expr::Const(0.0),
        }
    }

    pub fn eval(&self, ctx: &Context) -> f32 {
        match self {
            Expr::Const(v) => *v,
            Expr::AnimTime => ctx.anim_time,
            Expr::LifeTime => ctx.life_time,
            Expr::Neg(e) => -e.eval(ctx),
            Expr::Add(a, b) => a.eval(ctx) + b.eval(ctx),
            Expr::Sub(a, b) => a.eval(ctx) - b.eval(ctx),
            Expr::Mul(a, b) => a.eval(ctx) * b.eval(ctx),
            Expr::Div(a, b) => {
                let d = b.eval(ctx);
                if d == 0.0 {
                    0.0
                } else {
                    a.eval(ctx) / d
                }
            }
            // Molang trig operates in degrees.
            Expr::Sin(e) => e.eval(ctx).to_radians().sin(),
            Expr::Cos(e) => e.eval(ctx).to_radians().cos(),
            Expr::Abs(e) => e.eval(ctx).abs(),
            Expr::Mod(a, b) => {
                let d = b.eval(ctx);
                if d == 0.0 {
                    0.0
                } else {
                    a.eval(ctx).rem_euclid(d)
                }
            }
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "molang-expr")
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f32),
    Ident(String),
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    Comma,
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn new(src: &str) -> Self {
        Self {
            toks: tokenize(src),
            pos: 0,
        }
    }

    fn parse(&mut self) -> Option<Expr> {
        if self.toks.is_empty() {
            return None;
        }
        let e = self.expr()?;
        Some(e)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn expr(&mut self) -> Option<Expr> {
        let mut left = self.term()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Plus => {
                    self.next();
                    let right = self.term()?;
                    left = Expr::Add(Box::new(left), Box::new(right));
                }
                Tok::Minus => {
                    self.next();
                    let right = self.term()?;
                    left = Expr::Sub(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Some(left)
    }

    fn term(&mut self) -> Option<Expr> {
        let mut left = self.unary()?;
        while let Some(op) = self.peek() {
            match op {
                Tok::Star => {
                    self.next();
                    let right = self.unary()?;
                    left = Expr::Mul(Box::new(left), Box::new(right));
                }
                Tok::Slash => {
                    self.next();
                    let right = self.unary()?;
                    left = Expr::Div(Box::new(left), Box::new(right));
                }
                _ => break,
            }
        }
        Some(left)
    }

    fn unary(&mut self) -> Option<Expr> {
        if matches!(self.peek(), Some(Tok::Minus)) {
            self.next();
            let e = self.unary()?;
            return Some(Expr::Neg(Box::new(e)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Option<Expr> {
        match self.next()? {
            Tok::Num(v) => Some(Expr::Const(v)),
            Tok::LParen => {
                let e = self.expr()?;
                if matches!(self.peek(), Some(Tok::RParen)) {
                    self.next();
                }
                Some(e)
            }
            Tok::Ident(name) => self.ident(name),
            _ => None,
        }
    }

    fn ident(&mut self, name: String) -> Option<Expr> {
        let lname = name.to_ascii_lowercase();
        // Function call form: `math.sin(...)`.
        if matches!(self.peek(), Some(Tok::LParen)) {
            self.next();
            let arg = self.expr().unwrap_or(Expr::Const(0.0));
            // Optional second arg for math.mod.
            let arg2 = if matches!(self.peek(), Some(Tok::Comma)) {
                self.next();
                self.expr()
            } else {
                None
            };
            if matches!(self.peek(), Some(Tok::RParen)) {
                self.next();
            }
            return Some(match lname.as_str() {
                "math.sin" => Expr::Sin(Box::new(arg)),
                "math.cos" => Expr::Cos(Box::new(arg)),
                "math.abs" => Expr::Abs(Box::new(arg)),
                "math.mod" => Expr::Mod(Box::new(arg), Box::new(arg2.unwrap_or(Expr::Const(1.0)))),
                _ => Expr::Const(0.0),
            });
        }
        // Variable/query form.
        Some(match lname.as_str() {
            "query.anim_time" | "q.anim_time" => Expr::AnimTime,
            "query.life_time" | "q.life_time" => Expr::LifeTime,
            "math.pi" => Expr::Const(std::f32::consts::PI),
            _ => Expr::Const(0.0),
        })
    }
}

fn tokenize(src: &str) -> Vec<Tok> {
    let mut toks = Vec::new();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' | '\n' | '\r' | ';' => {
                i += 1;
            }
            '+' => {
                toks.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                toks.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                toks.push(Tok::Star);
                i += 1;
            }
            '/' => {
                toks.push(Tok::Slash);
                i += 1;
            }
            '(' => {
                toks.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                toks.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                toks.push(Tok::Comma);
                i += 1;
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                if let Ok(v) = s.parse::<f32>() {
                    toks.push(Tok::Num(v));
                }
            }
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '.')
                {
                    i += 1;
                }
                let s: String = chars[start..i].iter().collect();
                toks.push(Tok::Ident(s));
            }
            _ => {
                i += 1;
            }
        }
    }
    toks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(src: &str, t: f32) -> f32 {
        Expr::parse(src).eval(&Context {
            anim_time: t,
            life_time: t,
        })
    }

    #[test]
    fn arithmetic_precedence() {
        assert!((eval("2 + 3 * 4", 0.0) - 14.0).abs() < 1e-4);
        assert!((eval("(2 + 3) * 4", 0.0) - 20.0).abs() < 1e-4);
        assert!((eval("-5 + 2", 0.0) - -3.0).abs() < 1e-4);
    }

    #[test]
    fn trig_is_degrees() {
        assert!((eval("math.sin(90)", 0.0) - 1.0).abs() < 1e-4);
        assert!((eval("math.cos(0)", 0.0) - 1.0).abs() < 1e-4);
    }

    #[test]
    fn anim_time_query() {
        // Classic vanilla idle arm sway shape.
        let v = eval("math.cos(query.anim_time * 38.17) * 13", 0.5);
        let expected = (0.5f32 * 38.17).to_radians().cos() * 13.0;
        assert!((v - expected).abs() < 1e-3);
    }

    #[test]
    fn unknown_query_is_zero() {
        assert_eq!(eval("query.some_unknown_thing", 1.0), 0.0);
    }

    #[test]
    fn division_by_zero_is_zero() {
        assert_eq!(eval("5 / 0", 0.0), 0.0);
    }
}
