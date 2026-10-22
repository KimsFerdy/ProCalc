//! Parses the LaTeX that MathLive produces (plus plain typed text like `sqrt(2)*pi`)
//! into an [`Expr`] tree.

use crate::EngineError;
use crate::ast::{BinOp, Const, Expr, Func, SymOp};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Digit(char),
    Dot,
    Letter(char),
    /// A LaTeX command without the backslash, e.g. `frac`.
    Cmd(String),
    Sym(char),
}

/// Greek letters usable as variable names (`\\pi` is the constant).
const GREEK: &[&str] = &[
    "alpha", "beta", "gamma", "delta", "epsilon", "varepsilon", "zeta", "eta", "theta",
    "vartheta", "iota", "kappa", "lambda", "mu", "nu", "xi", "rho", "sigma", "tau", "upsilon",
    "phi", "varphi", "chi", "psi", "omega", "Gamma", "Delta", "Theta", "Lambda", "Xi", "Phi",
    "Psi", "Omega", "ell",
];

/// Commands that only affect spacing or delimiter sizing.
const IGNORED_CMDS: &[&str] = &[
    ",", ";", ":", "!", " ", "quad", "qquad", "left", "right", "mleft", "mright", "bigl", "bigr",
    "Bigl", "Bigr", "biggl", "biggr", "big", "Big", "displaystyle", "textstyle",
];

fn tokenize(src: &str) -> Result<Vec<(Tok, usize)>, EngineError> {
    let chars: Vec<(usize, char)> = src.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        i += 1;
        let tok = match c {
            c if c.is_whitespace() => continue,
            '0'..='9' => Tok::Digit(c),
            '.' => Tok::Dot,
            'π' => Tok::Cmd("pi".into()),
            '×' | '·' | '⋅' | '*' => Tok::Sym('*'),
            '÷' | '/' => Tok::Sym('/'),
            '−' | '-' => Tok::Sym('-'),
            '°' => Tok::Cmd("degree".into()),
            '√' => Tok::Cmd("sqrt".into()),
            '\\' => {
                let start = i;
                if i < chars.len() && !chars[i].1.is_ascii_alphabetic() {
                    // Single-symbol command such as `\,` or `\{`.
                    i += 1;
                } else {
                    while i < chars.len() && chars[i].1.is_ascii_alphabetic() {
                        i += 1;
                    }
                }
                let name: String = chars[start..i].iter().map(|(_, c)| *c).collect();
                if name.is_empty() {
                    return Err(EngineError::at("Stray backslash", pos));
                }
                if IGNORED_CMDS.contains(&name.as_str()) {
                    continue;
                }
                match name.as_str() {
                    "{" | "lbrace" => Tok::Sym('('),
                    "}" | "rbrace" => Tok::Sym(')'),
                    "lbrack" => Tok::Sym('('),
                    "rbrack" => Tok::Sym(')'),
                    "vert" | "lvert" | "rvert" | "mid" | "|" => Tok::Sym('|'),
                    "cdot" | "times" | "ast" => Tok::Sym('*'),
                    "div" => Tok::Sym('/'),
                    "%" => Tok::Sym('%'),
                    _ => Tok::Cmd(name),
                }
            }
            c if c.is_alphabetic() => Tok::Letter(c),
            '+' | '^' | '_' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '!' | '%' => Tok::Sym(c),
            _ => return Err(EngineError::at(format!("Unexpected symbol '{c}'"), pos)),
        };
        out.push((tok, pos));
    }
    Ok(out)
}

pub fn parse(src: &str) -> Result<Expr, EngineError> {
    let toks = tokenize(src)?;
    if toks.is_empty() {
        return Err(EngineError::new("Nothing to calculate"));
    }
    let mut p = Parser { toks, i: 0, end: src.len() };
    let e = p.expr()?;
    if p.i < p.toks.len() {
        return Err(EngineError::at("Unexpected input", p.pos()));
    }
    Ok(e)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    i: usize,
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i).map(|(t, _)| t)
    }

    fn pos(&self) -> usize {
        self.toks.get(self.i).map(|(_, p)| *p).unwrap_or(self.end)
    }

    fn eat_sym(&mut self, c: char) -> bool {
        if self.peek() == Some(&Tok::Sym(c)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn expect_sym(&mut self, c: char) -> Result<(), EngineError> {
        if self.eat_sym(c) {
            Ok(())
        } else {
            Err(EngineError::at(format!("Expected '{c}'"), self.pos()))
        }
    }

    fn eat_cmd(&mut self, name: &str) -> bool {
        if matches!(self.peek(), Some(Tok::Cmd(n)) if n == name) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    // expr := term (('+' | '-') term)*
    fn expr(&mut self) -> Result<Expr, EngineError> {
        let mut lhs = self.term()?;
        loop {
            let op = if self.eat_sym('+') {
                BinOp::Add
            } else if self.eat_sym('-') {
                BinOp::Sub
            } else {
                return Ok(lhs);
            };
            let rhs = self.term()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
    }

    // term := unary (('*' | '/') unary | <implicit> power)*
    fn term(&mut self) -> Result<Expr, EngineError> {
        let mut lhs = self.unary()?;
        loop {
            let (op, rhs) = if self.eat_sym('*') {
                (BinOp::Mul, self.unary()?)
            } else if self.eat_sym('/') {
                (BinOp::Div, self.unary()?)
            } else if self.starts_implicit_factor() {
                (BinOp::Mul, self.power()?)
            } else {
                return Ok(lhs);
            };
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
    }

    fn starts_implicit_factor(&self) -> bool {
        match self.peek() {
            Some(Tok::Digit(_) | Tok::Dot | Tok::Letter(_)) => true,
            Some(Tok::Sym('(' | '[' | '{')) => true,
            Some(Tok::Cmd(name)) => !matches!(name.as_str(), "degree" | "circ" | "placeholder"),
            _ => false,
        }
    }

    fn unary(&mut self) -> Result<Expr, EngineError> {
        if self.eat_sym('-') {
            return Ok(Expr::Neg(Box::new(self.unary()?)));
        }
        if self.eat_sym('+') {
            return self.unary();
        }
        self.power()
    }

    // power := postfix ('^' exponent)?   (right-associative through braces)
    fn power(&mut self) -> Result<Expr, EngineError> {
        let base = self.postfix()?;
        if !self.eat_sym('^') {
            return Ok(base);
        }
        match self.exponent()? {
            Exponent::Degree => Ok(degrees(base)),
            Exponent::Value(e) => {
                let p = Expr::Pow(Box::new(base), Box::new(e));
                self.postfix_ops(p)
            }
        }
    }

    fn exponent(&mut self) -> Result<Exponent, EngineError> {
        if self.peek() == Some(&Tok::Sym('{')) {
            let save = self.i;
            self.i += 1;
            if self.eat_cmd("circ") && self.eat_sym('}') {
                return Ok(Exponent::Degree);
            }
            self.i = save;
            return Ok(Exponent::Value(self.braced()?));
        }
        if self.eat_cmd("circ") {
            return Ok(Exponent::Degree);
        }
        // Unbraced exponent: LaTeX takes exactly one token, e.g. `2^10` is 2¹·0.
        if self.eat_sym('-') {
            return Ok(Exponent::Value(Expr::Neg(Box::new(self.single_token_atom()?))));
        }
        Ok(Exponent::Value(self.single_token_atom()?))
    }

    fn single_token_atom(&mut self) -> Result<Expr, EngineError> {
        match self.peek() {
            Some(Tok::Digit(d)) => {
                let d = *d;
                self.i += 1;
                Ok(Expr::Number(d.to_string()))
            }
            Some(Tok::Sym('(' | '{')) | Some(Tok::Cmd(_)) | Some(Tok::Letter(_)) => self.primary(),
            _ => Err(EngineError::at("Missing exponent", self.pos())),
        }
    }

    fn postfix(&mut self) -> Result<Expr, EngineError> {
        let e = self.primary()?;
        self.postfix_ops(e)
    }

    fn postfix_ops(&mut self, mut e: Expr) -> Result<Expr, EngineError> {
        loop {
            if self.eat_sym('!') {
                // `n!!` is the double factorial n·(n−2)·(n−4)…, not (n!)!
                let step = if self.eat_sym('!') { 2 } else { 1 };
                e = Expr::Factorial(Box::new(e), step);
            } else if self.eat_sym('%') {
                e = Expr::Percent(Box::new(e));
            } else if self.eat_cmd("degree") {
                e = degrees(e);
            } else {
                return Ok(e);
            }
        }
    }

    fn braced(&mut self) -> Result<Expr, EngineError> {
        self.expect_sym('{')?;
        let e = self.expr()?;
        self.expect_sym('}')?;
        Ok(e)
    }

    /// Argument of `\frac`/`\sqrt`: a braced group, or a single token (`\frac12`).
    fn group_arg(&mut self) -> Result<Expr, EngineError> {
        if self.peek() == Some(&Tok::Sym('{')) {
            self.braced()
        } else {
            self.single_token_atom()
        }
    }

    fn number(&mut self) -> Result<Expr, EngineError> {
        let start = self.pos();
        let mut s = String::new();
        let mut seen_dot = false;
        while let Some(t) = self.peek() {
            match t {
                Tok::Digit(d) => s.push(*d),
                Tok::Dot if !seen_dot => {
                    seen_dot = true;
                    s.push('.');
                }
                Tok::Dot => return Err(EngineError::at("Number has two decimal points", self.pos())),
                _ => break,
            }
            self.i += 1;
        }
        if s == "." {
            return Err(EngineError::at("A lone '.' is not a number", start));
        }
        Ok(Expr::Number(s))
    }

    fn primary(&mut self) -> Result<Expr, EngineError> {
        let pos = self.pos();
        match self.peek().cloned() {
            None => Err(EngineError::at("Expression ends too early", pos)),
            Some(Tok::Digit(_) | Tok::Dot) => self.number(),
            Some(Tok::Sym(open @ ('(' | '[' | '{'))) => {
                self.i += 1;
                let e = self.expr()?;
                let close = match open {
                    '(' => ')',
                    '[' => ']',
                    _ => '}',
                };
                self.expect_sym(close)?;
                Ok(e)
            }
            Some(Tok::Sym('|')) => {
                self.i += 1;
                let e = self.expr()?;
                self.expect_sym('|')?;
                Ok(Expr::Call(Func::Abs, Box::new(e)))
            }
            Some(Tok::Letter(_)) => self.word(),
            Some(Tok::Cmd(name)) => {
                self.i += 1;
                self.command(&name, pos)
            }
            Some(Tok::Sym(c)) => Err(EngineError::at(format!("Unexpected '{c}'"), pos)),
        }
    }

    /// Plain letters: known names like `sin`, `sqrt`, `pi`, else single-letter constants.
    fn word(&mut self) -> Result<Expr, EngineError> {
        let start = self.i;
        let pos = self.pos();
        let mut letters = String::new();
        while let Some(Tok::Letter(c)) = self.peek() {
            letters.push(*c);
            self.i += 1;
        }
        // Longest known name at the start of the run; the rest is parsed afterwards.
        for len in (2..=letters.len()).rev() {
            if !letters.is_char_boundary(len) {
                continue;
            }
            let name = &letters[..len];
            if name == "pi" || Func::from_name(name).is_some() || SymOp::from_name(name).is_some() {
                self.i = start + len;
                return self.named(name, pos);
            }
        }
        self.i = start + 1;
        let first = &letters[..letters.chars().next().unwrap().len_utf8()];
        self.named(first, pos)
    }

    fn command(&mut self, name: &str, pos: usize) -> Result<Expr, EngineError> {
        match name {
            "frac" | "dfrac" | "tfrac" | "cfrac" => {
                let num = self.group_arg()?;
                let den = self.group_arg()?;
                Ok(Expr::Binary(BinOp::Div, Box::new(num), Box::new(den)))
            }
            "sqrt" => {
                if self.eat_sym('[') {
                    let n = self.expr()?;
                    self.expect_sym(']')?;
                    let x = self.group_arg()?;
                    Ok(Expr::Root(Box::new(n), Box::new(x)))
                } else {
                    Ok(Expr::Call(Func::Sqrt, Box::new(self.group_arg()?)))
                }
            }
            "operatorname" | "mathrm" | "mathit" | "text" | "mathop" => {
                let word = self.braced_letters()?;
                self.named(&word, pos)
            }
            "placeholder" => Err(EngineError::at("Fill in the empty box first", pos)),
            "imaginaryI" | "imaginaryJ" => {
                Err(EngineError::at("Complex numbers aren't supported yet", pos))
            }
            "infty" => Err(EngineError::at("Infinity isn't a number you can calculate with", pos)),
            _ => self.named(name, pos),
        }
    }

    fn braced_letters(&mut self) -> Result<String, EngineError> {
        self.expect_sym('{')?;
        let mut s = String::new();
        while let Some(Tok::Letter(c)) = self.peek() {
            s.push(*c);
            self.i += 1;
        }
        self.expect_sym('}')?;
        Ok(s)
    }

    fn named(&mut self, name: &str, pos: usize) -> Result<Expr, EngineError> {
        match name {
            "pi" => return Ok(Expr::Const(Const::Pi)),
            "e" | "exponentialE" => return Ok(Expr::Const(Const::E)),
            _ => {}
        }
        if let Some(f) = Func::from_name(name) {
            return self.call(f);
        }
        if let Some(op) = SymOp::from_name(name) {
            return Ok(Expr::Op(op, Box::new(self.power()?)));
        }
        let var = if GREEK.contains(&name) {
            format!("\\{name}")
        } else if name.chars().count() == 1 {
            name.to_string()
        } else {
            return Err(EngineError::at(format!("Unknown function '{name}'"), pos));
        };
        Ok(Expr::Var(match self.subscript()? {
            Some(sub) => format!("{var}_{{{sub}}}"),
            None => var,
        }))
    }

    /// Raw LaTeX of a variable subscript: `x_1`, `x_{12}`, `v_{\max}`.
    fn subscript(&mut self) -> Result<Option<String>, EngineError> {
        if !self.eat_sym('_') {
            return Ok(None);
        }
        let mut out = String::new();
        let braced = self.eat_sym('{');
        let mut depth = 0;
        loop {
            let pos = self.pos();
            let Some((tok, _)) = self.toks.get(self.i).cloned() else {
                return Err(EngineError::at("Unfinished subscript", pos));
            };
            self.i += 1;
            match tok {
                Tok::Sym('{') => depth += 1,
                Tok::Sym('}') if braced && depth == 0 => break,
                Tok::Sym('}') => depth -= 1,
                _ => {}
            }
            match tok {
                Tok::Digit(c) | Tok::Letter(c) | Tok::Sym(c) => out.push(c),
                Tok::Dot => out.push('.'),
                Tok::Cmd(n) => {
                    out.push('\\');
                    out.push_str(&n);
                    out.push(' ');
                }
            }
            if !braced {
                break;
            }
        }
        let out = out.trim().to_string();
        if out.is_empty() {
            return Err(EngineError::at("Empty subscript", self.pos()));
        }
        Ok(Some(out))
    }

    /// Function application. Handles `\sin^{2}x`, `\sin^{-1}x`, `\log_{2}8`,
    /// and both `\sin(x)` and `\sin x`.
    fn call(&mut self, mut f: Func) -> Result<Expr, EngineError> {
        let mut base = None;
        let mut power = None;
        for _ in 0..2 {
            if self.eat_sym('_') {
                base = Some(self.group_arg()?);
            } else if self.eat_sym('^') {
                match self.exponent()? {
                    Exponent::Value(e) => power = Some(e),
                    Exponent::Degree => {
                        return Err(EngineError::at("Unexpected degree sign", self.pos()));
                    }
                }
            }
        }
        if let Some(Expr::Neg(inner)) = &power
            && **inner == Expr::Number("1".into())
                && let Some(inv) = inverse(f) {
                    f = inv;
                    power = None;
                }
        // `\sin(x)^2` is (sin x)², while `\sin x^2` is sin(x²).
        let bracketed = self.peek() == Some(&Tok::Sym('('));
        let arg = if bracketed { self.primary()? } else { self.power()? };
        let mut e = match (f, base) {
            (Func::Log, Some(b)) => Expr::LogBase(Box::new(b), Box::new(arg)),
            (_, Some(_)) => return Err(EngineError::at("Only log takes a subscript base", self.pos())),
            (f, None) => Expr::Call(f, Box::new(arg)),
        };
        if let Some(p) = power {
            e = Expr::Pow(Box::new(e), Box::new(p));
        }
        if bracketed && self.eat_sym('^') {
            match self.exponent()? {
                Exponent::Value(p) => e = Expr::Pow(Box::new(e), Box::new(p)),
                Exponent::Degree => return Err(EngineError::at("Unexpected degree sign", self.pos())),
            }
        }
        Ok(e)
    }
}

enum Exponent {
    Value(Expr),
    Degree,
}

fn degrees(e: Expr) -> Expr {
    let factor = Expr::Binary(
        BinOp::Div,
        Box::new(Expr::Const(Const::Pi)),
        Box::new(Expr::Number("180".into())),
    );
    Expr::Binary(BinOp::Mul, Box::new(e), Box::new(factor))
}

fn inverse(f: Func) -> Option<Func> {
    Some(match f {
        Func::Sin => Func::Asin,
        Func::Cos => Func::Acos,
        Func::Tan => Func::Atan,
        Func::Sinh => Func::Asinh,
        Func::Cosh => Func::Acosh,
        Func::Tanh => Func::Atanh,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num(s: &str) -> Box<Expr> {
        Box::new(Expr::Number(s.into()))
    }

    #[test]
    fn unbraced_exponent_takes_one_digit() {
        // 2^10 in LaTeX is 2¹ · 0
        let e = parse("2^10").unwrap();
        assert_eq!(
            e,
            Expr::Binary(BinOp::Mul, Box::new(Expr::Pow(num("2"), num("1"))), num("0"))
        );
    }

    #[test]
    fn frac_shorthand() {
        assert_eq!(parse("\\frac12").unwrap(), Expr::Binary(BinOp::Div, num("1"), num("2")));
    }

    #[test]
    fn plain_words() {
        assert!(parse("sqrt(2)*pi").is_ok());
        assert!(parse("sin(30)+cos(60)").is_ok());
        assert!(matches!(parse("x+1"), Ok(Expr::Binary(BinOp::Add, _, _))));
    }

    #[test]
    fn variables() {
        assert_eq!(parse("\\theta").unwrap(), Expr::Var("\\theta".into()));
        assert_eq!(parse("x_1").unwrap(), Expr::Var("x_{1}".into()));
        assert_eq!(parse("v_{max}").unwrap(), Expr::Var("v_{max}".into()));
        assert_eq!(parse("v_{\\max}").unwrap(), Expr::Var("v_{\\max}".into()));
        // Letters multiply: xy = x·y
        assert_eq!(
            parse("xy").unwrap(),
            Expr::Binary(BinOp::Mul, Box::new(Expr::Var("x".into())), Box::new(Expr::Var("y".into())))
        );
        assert!(parse("\\foo").is_err());
    }

    #[test]
    fn mathlive_output() {
        for s in [
            "\\frac{1}{3}+\\sqrt{2}\\cdot\\sin\\left(\\frac{\\pi}{6}\\right)",
            "\\sqrt[3]{27}",
            "5!",
            "\\left\\vert-3\\right\\vert",
            "\\log_{2}\\left(8\\right)",
            "\\sin^{-1}\\left(0.5\\right)",
            "30^{\\circ}",
            "2\\pi",
            "\\operatorname{abs}(-2)",
            "\\operatorname{expand}\\left((x+1)^{2}\\right)",
            "factor(x^2-1)",
            "\\exponentialE^{2}",
            "1\\,000\\,000",
        ] {
            assert!(parse(s).is_ok(), "failed: {s}: {:?}", parse(s));
        }
    }

    #[test]
    fn function_power_binds_outside_brackets() {
        let sin_x = Expr::Call(Func::Sin, Box::new(Expr::Var("x".into())));
        let two = Box::new(Expr::Number("2".into()));
        assert_eq!(parse("\\sin(x)^2").unwrap(), Expr::Pow(Box::new(sin_x.clone()), two.clone()));
        assert_eq!(parse("\\sin^{2}x").unwrap(), Expr::Pow(Box::new(sin_x), two.clone()));
        let x2 = Expr::Pow(Box::new(Expr::Var("x".into())), two);
        assert_eq!(parse("\\sin x^2").unwrap(), Expr::Call(Func::Sin, Box::new(x2)));
    }

    #[test]
    fn placeholder_is_an_error() {
        assert!(parse("\\frac{1}{\\placeholder{}}").is_err());
    }
}
