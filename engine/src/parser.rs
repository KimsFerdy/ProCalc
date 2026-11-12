//! Parses the LaTeX that MathLive produces (plus plain typed text like `sqrt(2)*pi`)
//! into an [`Expr`] tree.

use std::collections::BTreeSet;

use crate::ast::{Arg, BinOp, Chain, Const, DivPart, Expr, Func, GeoFn, Goal, MatFn, NumFn, Rel, StatFn, Stmt, SymOp};
use crate::{EngineError, units};

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Digit(char),
    Dot,
    Letter(char),
    /// A LaTeX command without the backslash, e.g. `frac`.
    Cmd(String),
    Sym(char),
    /// `:=`, `\coloneq`, `≔`
    Assign,
    /// `\to`, `→`: unit conversion.
    Arrow,
    /// `<`, `\le`, `>`, `\ge` (and ≤ ≥).
    Rel(Rel),
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
    "Bigl", "Bigr", "biggl", "biggr", "big", "Big", "displaystyle", "textstyle", "limits", "nolimits",
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
            '×' => Tok::Sym('×'),
            '·' | '⋅' | '*' => Tok::Sym('*'),
            '÷' | '/' => Tok::Sym('/'),
            // Typed -> is an arrow: 60 mph -> km/h, lim_{x->0}.
            '-' if chars.get(i).is_some_and(|(_, c)| *c == '>') => {
                i += 1;
                Tok::Arrow
            }
            '−' | '-' => Tok::Sym('-'),
            '°' => Tok::Cmd("degree".into()),
            '√' => Tok::Cmd("sqrt".into()),
            '≔' => Tok::Assign,
            '→' => Tok::Arrow,
            '<' => Tok::Rel(Rel::Lt),
            '>' => Tok::Rel(Rel::Gt),
            '≤' => Tok::Rel(Rel::Le),
            '≥' => Tok::Rel(Rel::Ge),
            // Typed or pasted symbols, as their LaTeX commands.
            '∫' => Tok::Cmd("int".into()),
            '∞' => Tok::Cmd("infty".into()),
            '∑' => Tok::Cmd("sum".into()),
            '∏' => Tok::Cmd("prod".into()),
            'ⅆ' => Tok::Letter('d'),
            // Superscript digits: x² is x^2.
            '⁰' | '¹' | '²' | '³' | '⁴' | '⁵' | '⁶' | '⁷' | '⁸' | '⁹' => {
                out.push((Tok::Sym('^'), pos));
                let d = match c {
                    '⁰' => '0',
                    '¹' => '1',
                    '²' => '2',
                    '³' => '3',
                    '⁴' => '4',
                    '⁵' => '5',
                    '⁶' => '6',
                    '⁷' => '7',
                    '⁸' => '8',
                    _ => '9',
                };
                Tok::Digit(d)
            }
            '℃' => Tok::Cmd("celsius".into()),
            '℉' => Tok::Cmd("fahrenheit".into()),
            ':' if chars.get(i).is_some_and(|(_, c)| *c == '=') => {
                i += 1;
                Tok::Assign
            }
            // `=` only belongs in |_{x=3}; anywhere else the parser explains :=.
            '=' => Tok::Sym('='),
            '\'' | '′' => Tok::Sym('\''),
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
                    // `\left.` is an invisible delimiter, as in \left. x^2 \right|_{x=3}.
                    if matches!(name.as_str(), "left" | "right") && chars.get(i).is_some_and(|(_, c)| *c == '.') {
                        i += 1;
                    }
                    continue;
                }
                match name.as_str() {
                    "{" | "lbrace" => Tok::Sym('('),
                    "}" | "rbrace" => Tok::Sym(')'),
                    "lbrack" => Tok::Sym('('),
                    "rbrack" => Tok::Sym(')'),
                    "vert" | "lvert" | "rvert" | "mid" | "|" => Tok::Sym('|'),
                    "cdot" | "ast" => Tok::Sym('*'),
                    // Between vectors, × is the cross product.
                    "times" => Tok::Sym('×'),
                    "div" => Tok::Sym('/'),
                    "%" => Tok::Sym('%'),
                    "coloneq" | "coloneqq" => Tok::Assign,
                    "to" | "rightarrow" | "longrightarrow" => Tok::Arrow,
                    "lt" => Tok::Rel(Rel::Lt),
                    "gt" => Tok::Rel(Rel::Gt),
                    "le" | "leq" | "leqslant" => Tok::Rel(Rel::Le),
                    "ge" | "geq" | "geqslant" => Tok::Rel(Rel::Ge),
                    // MathLive's upright differential d, as in \int x\,\differentialD x.
                    "differentialD" => Tok::Letter('d'),
                    _ => Tok::Cmd(name),
                }
            }
            c if c.is_alphabetic() => Tok::Letter(c),
            '+' | '^' | '_' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '!' | '%' | ',' | ';' => Tok::Sym(c),
            // Column separator: matrix entries, or alignment in \begin{cases} (dropped there).
            '&' => Tok::Sym('&'),
            _ => return Err(EngineError::at(format!("Unexpected symbol '{c}'"), pos)),
        };
        out.push((tok, pos));
    }
    Ok(out)
}

/// Parses a plain expression; definitions are rejected.
pub fn parse(src: &str) -> Result<Expr, EngineError> {
    match parse_statement(src, &BTreeSet::new())? {
        Stmt::Expr(e) => Ok(e),
        Stmt::Define { .. } => Err(EngineError::new("Expected an expression, not a definition")),
        Stmt::Equations(..) => Err(EngineError::new("Expected an expression, not an equation")),
        Stmt::Inequalities(_) => Err(EngineError::new("Expected an expression, not an inequality")),
    }
}

/// Most equations in one system.
pub const MAX_EQUATIONS: usize = 10;

fn goal_of(name: &str) -> Option<Goal> {
    Some(match name {
        "minimize" | "minimise" => Goal::Min,
        "maximize" | "maximise" => Goal::Max,
        "extrema" | "optimize" | "optimise" => Goal::All,
        _ => return None,
    })
}

/// A name that is a function only with a bracket after it: gcd(…), mean(…), circle(…).
fn bracket_name(name: &str) -> bool {
    NumFn::from_name(name).is_some()
        || DivPart::from_name(name).is_some()
        || MatFn::from_name(name).is_some()
        || StatFn::from_name(name).is_some()
        || GeoFn::from_name(name).is_some()
        || goal_of(name).is_some()
        || matches!(name, "taylor" | "series")
}

/// Matrix environments, kept as `\pmatrix{a, b; c, d}` for the parser.
const MATRIX_ENVS: &[&str] = &["pmatrix", "bmatrix", "Bmatrix", "matrix", "smallmatrix", "vmatrix", "Vmatrix"];

/// Drops `\begin{cases}` / `\end{cases}` (any other environment name) and turns the `\\`
/// line breaks inside them into `;`, so a cases block reads like `x+y=3; x-y=1`. A matrix
/// environment becomes `\pmatrix{1, 2; 3, 4}`: `&` between entries, `\\` between rows.
fn flatten_environments(toks: Vec<(Tok, usize)>) -> Vec<(Tok, usize)> {
    let mut out = Vec::with_capacity(toks.len());
    let mut stack: Vec<bool> = Vec::new(); // true inside a matrix
    let mut k = 0;
    while k < toks.len() {
        match &toks[k].0 {
            Tok::Cmd(c) if c == "begin" || c == "end" => {
                let begin = c == "begin";
                let pos = toks[k].1;
                k += 1;
                let mut name = String::new();
                if matches!(toks.get(k), Some((Tok::Sym('{'), _))) {
                    while k < toks.len() && toks[k].0 != Tok::Sym('}') {
                        if let Tok::Letter(ch) = toks[k].0 {
                            name.push(ch);
                        }
                        k += 1;
                    }
                }
                let matrix = MATRIX_ENVS.contains(&name.as_str());
                if begin {
                    stack.push(matrix);
                    if matrix {
                        out.push((Tok::Cmd(name), pos));
                        out.push((Tok::Sym('{'), pos));
                    }
                } else if stack.pop() == Some(true) {
                    out.push((Tok::Sym('}'), pos));
                }
            }
            Tok::Cmd(c) if c == "\\" || c == "cr" => out.push((Tok::Sym(';'), toks[k].1)),
            Tok::Sym('&') if stack.last() == Some(&true) => out.push((Tok::Sym(','), toks[k].1)),
            Tok::Sym('&') => {}
            _ => out.push(toks[k].clone()),
        }
        k += 1;
    }
    out
}

/// Splits tokens at bracket depth 0 wherever `is_sep` holds.
fn split_top(toks: &[(Tok, usize)], is_sep: impl Fn(&Tok) -> bool) -> Vec<Vec<(Tok, usize)>> {
    let mut parts = vec![Vec::new()];
    let mut depth = 0i32;
    for t in toks {
        match t.0 {
            Tok::Sym('(' | '[' | '{') => depth += 1,
            Tok::Sym(')' | ']' | '}') => depth -= 1,
            _ => {}
        }
        if depth == 0 && is_sep(&t.0) {
            parts.push(Vec::new());
        } else {
            parts.last_mut().expect("starts with one part").push(t.clone());
        }
    }
    parts
}

/// Equations and comparisons separated by `,` or `;` (or the rows of a cases block):
/// `2x+1=7`, `x+y=3, x-y=1`, `x^2<4`, `x=\cos t, y=\sin t, 0\le t\le 4\pi`.
fn equations(toks: Vec<(Tok, usize)>, end: usize, funcs: &BTreeSet<String>) -> Result<Stmt, EngineError> {
    let pieces: Vec<_> = split_top(&toks, |t| matches!(t, Tok::Sym(',' | ';')))
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect();
    if pieces.len() > MAX_EQUATIONS + 4 {
        return Err(EngineError::new(format!("Systems can have up to {MAX_EQUATIONS} equations for now")));
    }
    let mut eqs = Vec::new();
    let mut chains = Vec::new();
    for piece in pieces {
        let at = piece[0].1;
        let piece_end = piece.last().map(|t| t.1 + 1).unwrap_or(end);
        let sides = split_top(&piece, |t| *t == Tok::Sym('='));
        let has_rel = split_top(&piece, |t| matches!(t, Tok::Rel(_))).len() > 1;
        if sides.len() == 1 {
            if !has_rel {
                return Err(EngineError::at("Each part needs an = or a comparison like <", at));
            }
            chains.push(chain(&piece, piece_end, funcs)?);
            continue;
        }
        if has_rel {
            return Err(EngineError::at("Use either = or a comparison in each part, not both", at));
        }
        let [lhs, rhs] = sides.as_slice() else {
            return Err(EngineError::at("Only one = per equation", at));
        };
        if lhs.is_empty() || rhs.is_empty() {
            return Err(EngineError::at("An equation needs something on both sides of =", at));
        }
        let l = Parser::new(lhs.clone(), rhs[0].1, funcs).whole_expr()?;
        let r = Parser::new(rhs.clone(), piece_end, funcs).whole_expr()?;
        eqs.push((l, r));
    }
    if eqs.len() > MAX_EQUATIONS {
        return Err(EngineError::new(format!("Systems can have up to {MAX_EQUATIONS} equations for now")));
    }
    Ok(if eqs.is_empty() { Stmt::Inequalities(chains) } else { Stmt::Equations(eqs, chains) })
}

/// `a < b` or `0 \le t \le 4\pi`.
fn chain(piece: &[(Tok, usize)], end: usize, funcs: &BTreeSet<String>) -> Result<Chain, EngineError> {
    let rels: Vec<Rel> = piece.iter().filter_map(|(t, _)| if let Tok::Rel(r) = t { Some(*r) } else { None }).collect();
    let parts_toks = split_top(piece, |t| matches!(t, Tok::Rel(_)));
    if parts_toks.len() != rels.len() + 1 {
        return Err(EngineError::at("Comparisons can't go inside brackets", piece[0].1));
    }
    let mut parts = Vec::new();
    for (k, p) in parts_toks.iter().enumerate() {
        if p.is_empty() {
            return Err(EngineError::at("A comparison needs something on both sides", piece[0].1));
        }
        let p_end = parts_toks.get(k + 1).and_then(|n| n.first()).map(|t| t.1).unwrap_or(end);
        parts.push(Parser::new(p.clone(), p_end, funcs).whole_expr()?);
    }
    Ok(Chain { parts, rels })
}

/// Parses one line: an expression, or a definition `x := 5` / `f(x) := x^2`.
/// `funcs` names the user functions defined so far, so `f(2)` is a call and not f·2.
/// Letters used as the unknown function of a differential equation: written with a prime
/// (y', y'') without being a defined function, or as d/dx of themselves (\frac{dy}{dx}).
fn ode_names(toks: &[(Tok, usize)], funcs: &BTreeSet<String>) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let t = |k: usize| toks.get(k).map(|(t, _)| t);
    for k in 0..toks.len() {
        if let (Some(Tok::Letter(c)), Some(Tok::Sym('\''))) = (t(k), t(k + 1)) {
            // Not the end of a word like "xy'": only the letter right before the prime.
            if !funcs.contains(&c.to_string()) {
                out.insert(c.to_string());
            }
        }
        // \frac{dy}{dx} or \frac{d^2y}{dx^2}
        if t(k) == Some(&Tok::Cmd("frac".into())) && t(k + 1) == Some(&Tok::Sym('{')) && t(k + 2) == Some(&Tok::Letter('d')) {
            let mut j = k + 3;
            if t(j) == Some(&Tok::Sym('^')) {
                j += 2;
            }
            if let (Some(Tok::Letter(c)), Some(Tok::Sym('}'))) = (t(j), t(j + 1))
                && *c != 'd'
                && t(j + 2) == Some(&Tok::Sym('{'))
                && t(j + 3) == Some(&Tok::Letter('d'))
            {
                out.insert(c.to_string());
            }
            // \frac{d}{dx}y in an equation: y is the unknown.
            let equation = toks.iter().any(|(t, _)| *t == Tok::Sym('='));
            if equation
                && t(k + 3) == Some(&Tok::Sym('}'))
                && t(k + 4) == Some(&Tok::Sym('{'))
                && t(k + 5) == Some(&Tok::Letter('d'))
                && let (Some(Tok::Letter(v)), Some(Tok::Sym('}')), Some(Tok::Letter(c))) = (t(k + 6), t(k + 7), t(k + 8))
                && c != v
            {
                out.insert(c.to_string());
            }
        }
    }
    out
}

pub fn parse_statement(src: &str, funcs: &BTreeSet<String>) -> Result<Stmt, EngineError> {
    let toks = flatten_environments(tokenize(src)?);
    if toks.is_empty() {
        return Err(EngineError::new("Nothing to calculate"));
    }
    // Unknown functions ride along in `funcs` as 'y, so every sub-parser sees them.
    let odes = ode_names(&toks, funcs);
    let extended: BTreeSet<String>;
    let funcs = if odes.is_empty() {
        funcs
    } else {
        extended = funcs.iter().cloned().chain(odes.iter().map(|n| format!("'{n}"))).collect();
        &extended
    };
    let assigns: Vec<usize> = toks.iter().enumerate().filter(|(_, (t, _))| *t == Tok::Assign).map(|(k, _)| k).collect();
    // An = outside brackets (not the one in |_{x=3}) makes an equation; <, ≤… an inequality.
    let has_equals = split_top(&toks, |t| *t == Tok::Sym('=')).len() > 1;
    // Outside brackets only: minimize(f, 0 ≤ x ≤ 5) is a call, not an inequality.
    let has_rel = split_top(&toks, |t| matches!(t, Tok::Rel(_))).len() > 1;
    if has_equals || has_rel {
        if !assigns.is_empty() {
            return Err(EngineError::new("Use := to define something and = or < for equations and inequalities, not both"));
        }
        return equations(toks, src.len(), funcs);
    }
    match assigns[..] {
        [] => Ok(Stmt::Expr(Parser::new(toks, src.len(), funcs).whole_expr()?)),
        [k] => {
            let assign_pos = toks[k].1;
            let mut rhs = toks;
            let lhs: Vec<_> = rhs.drain(..=k).take(k).collect();
            if lhs.is_empty() {
                return Err(EngineError::at("Put a name before := (as in x := 5)", assign_pos));
            }
            if rhs.is_empty() {
                return Err(EngineError::at("Put a value after :=", src.len()));
            }
            let (name, params) = Parser::new(lhs, assign_pos, funcs).definition_head()?;
            let body = Parser::new(rhs, src.len(), funcs).whole_expr()?;
            Ok(Stmt::Define { name, params, body })
        }
        [_, k, ..] => Err(EngineError::at("Only one := per line", toks[k].1)),
    }
}

/// Built-in names that a definition may not take over.
fn is_builtin(name: &str) -> bool {
    let plain = name.trim_start_matches('\\');
    // e_1 and i_1 are ordinary variables, so constants only clash without a subscript.
    let base = plain.split('_').next().unwrap_or(plain);
    matches!(plain, "pi" | "e" | "i" | "exponentialE" | "imaginaryI" | "imaginaryJ" | "infty")
        || base == "pi"
        || Func::from_name(base).is_some()
        || SymOp::from_name(base).is_some()
        || NumFn::from_name(base).is_some()
        || DivPart::from_name(base).is_some()
        || MatFn::from_name(base).is_some()
        || StatFn::from_name(base).is_some()
        || goal_of(base).is_some()
        || matches!(base, "taylor" | "series" | "lim" | "sum" | "prod")
}

struct Parser<'a> {
    toks: Vec<(Tok, usize)>,
    i: usize,
    end: usize,
    funcs: &'a BTreeSet<String>,
    /// After `→` and inside `\mathrm{km/h}`, letters spell unit names, not variables.
    unit_mode: bool,
    /// Inside a bound or a limit target, where `\infty` is allowed.
    allow_inf: bool,
}

impl<'a> Parser<'a> {
    fn new(toks: Vec<(Tok, usize)>, end: usize, funcs: &'a BTreeSet<String>) -> Parser<'a> {
        Parser { toks, i: 0, end, funcs, unit_mode: false, allow_inf: false }
    }

    /// Parses a bound or target, where ∞ may appear.
    fn with_inf<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, EngineError>) -> Result<T, EngineError> {
        let saved = std::mem::replace(&mut self.allow_inf, true);
        let out = f(self);
        self.allow_inf = saved;
        out
    }

    /// `\lim_{x\to a} body`, `\lim_{x\to 0^{+}}`, after the `\lim`.
    fn limit(&mut self, pos: usize) -> Result<Expr, EngineError> {
        let shape = || EngineError::at("Write a limit as \\lim_{x\\to a}", pos);
        if !self.eat_sym('_') || !self.eat_sym('{') {
            return Err(shape());
        }
        // The subscript's closing brace.
        let mut depth = 0;
        let mut close = None;
        for k in self.i..self.toks.len() {
            match self.toks[k].0 {
                Tok::Sym('{') => depth += 1,
                Tok::Sym('}') if depth == 0 => {
                    close = Some(k);
                    break;
                }
                Tok::Sym('}') => depth -= 1,
                _ => {}
            }
        }
        let close = close.ok_or_else(shape)?;
        let var = self.plain_var()?;
        if self.peek() != Some(&Tok::Arrow) {
            return Err(shape());
        }
        self.i += 1;
        // A side: 0^{+}, 0^+, 0^{-}
        let t: Vec<&Tok> = self.toks[self.i..close].iter().map(|(t, _)| t).collect();
        let sign = |t: &Tok| match t {
            Tok::Sym('+') => Some(1),
            Tok::Sym('-') => Some(-1),
            _ => None,
        };
        let (end, side) = match t.as_slice() {
            [.., Tok::Sym('^'), Tok::Sym('{'), s, Tok::Sym('}')] if sign(s).is_some() => (close - 4, sign(s).unwrap_or(0)),
            [.., Tok::Sym('^'), s] if sign(s).is_some() => (close - 2, sign(s).unwrap_or(0)),
            _ => (close, 0),
        };
        if end <= self.i {
            return Err(shape());
        }
        let mut sub = Parser::new(self.toks[self.i..end].to_vec(), self.toks[end].1, self.funcs);
        sub.allow_inf = true;
        let to = sub.whole_expr()?;
        self.i = close + 1;
        if self.peek().is_none() || matches!(self.peek(), Some(Tok::Sym(')' | '}' | ']'))) {
            return Err(EngineError::at("Write what to take the limit of after \\lim", self.pos()));
        }
        // Like an integral up to its dx, the body runs to the end: lim (√(x²+x) − x).
        let body = self.expr()?;
        Ok(Expr::Limit { body: Box::new(body), var, to: Box::new(to), side })
    }

    /// `\sum_{k=1}^{n} body` or `\prod_{k=1}^{n} body`, after the command.
    fn sum(&mut self, product: bool, pos: usize) -> Result<Expr, EngineError> {
        let name = if product { "\\prod" } else { "\\sum" };
        let shape = || EngineError::at(format!("Write it as {name}_{{k=1}}^{{n}}"), pos);
        if !self.eat_sym('_') || !self.eat_sym('{') {
            return Err(shape());
        }
        let var = self.plain_var()?;
        if !self.eat_sym('=') {
            return Err(shape());
        }
        let lo = self.with_inf(Self::expr)?;
        self.expect_sym('}')?;
        if !self.eat_sym('^') {
            return Err(shape());
        }
        let hi = self.with_inf(Self::group_arg)?;
        if self.peek().is_none() || matches!(self.peek(), Some(Tok::Sym(')' | '}' | ']'))) {
            return Err(EngineError::at(format!("Write what to add up after {name}"), self.pos()));
        }
        let body = self.expr()?;
        Ok(Expr::Sum { body: Box::new(body), var, lo: Box::new(lo), hi: Box::new(hi), product })
    }

    /// `taylor(f)`, `taylor(f, a)`, `taylor(f, a, n)` or `taylor(f, x, a, n)`.
    fn taylor(&mut self, pos: usize) -> Result<Expr, EngineError> {
        let shape = || EngineError::at("Write taylor(f, a, n): f about x = a up to the nth power", pos);
        if !self.eat_sym('(') {
            return Err(shape());
        }
        let mut args = vec![self.expr()?];
        while self.eat_sym(',') {
            args.push(self.expr()?);
        }
        self.expect_sym(')')?;
        let order = |e: &Expr| match e {
            Expr::Number(k) => k.parse::<u32>().ok().filter(|k| *k <= 40),
            _ => None,
        };
        let zero = || Box::new(Expr::Number("0".into()));
        let mut it = args.into_iter();
        let body = Box::new(it.next().ok_or_else(shape)?);
        let rest: Vec<Expr> = it.collect();
        Ok(match rest.as_slice() {
            [] => Expr::Taylor { body, var: None, at: zero(), order: 5 },
            [a] => Expr::Taylor { body, var: None, at: Box::new(a.clone()), order: 5 },
            [a, n] => Expr::Taylor { body, var: None, at: Box::new(a.clone()), order: order(n).ok_or_else(shape)? },
            [Expr::Var(x), a, n] => Expr::Taylor { body, var: Some(x.clone()), at: Box::new(a.clone()), order: order(n).ok_or_else(shape)? },
            _ => return Err(shape()),
        })
    }

    /// Parses with letters read as units.
    fn in_unit_mode<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, EngineError>) -> Result<T, EngineError> {
        let saved = std::mem::replace(&mut self.unit_mode, true);
        let out = f(self);
        self.unit_mode = saved;
        out
    }

    fn whole_expr(&mut self) -> Result<Expr, EngineError> {
        let mut e = self.expr()?;
        // Evaluation bars: x^2|_{x=3}, F|_{a}^{b}
        while self.peek() == Some(&Tok::Sym('|')) && self.toks.get(self.i + 1).map(|(t, _)| t) == Some(&Tok::Sym('_')) {
            self.i += 2;
            e = self.bar(e)?;
        }
        if self.peek() == Some(&Tok::Sym('=')) {
            return Err(EngineError::at("To define something, use := (as in x := 5)", self.pos()));
        }
        while self.peek() == Some(&Tok::Arrow) {
            self.i += 1;
            let target = self.in_unit_mode(Self::expr)?;
            e = Expr::Convert(Box::new(e), Box::new(target));
        }
        if self.i < self.toks.len() {
            return Err(EngineError::at("Unexpected input", self.pos()));
        }
        Ok(e)
    }

    /// The left side of `:=`: a variable (`x`, `\theta`, `v_{max}`) or a function head
    /// (`f(x)`, `area(w, h)`, `\operatorname{area}(w,h)`).
    fn definition_head(&mut self) -> Result<(String, Vec<String>), EngineError> {
        let pos = self.pos();
        let start = self.i;
        let mut letters = String::new();
        while let Some(Tok::Letter(c)) = self.peek() {
            letters.push(*c);
            self.i += 1;
        }
        let name = if letters.chars().count() > 1 {
            if is_builtin(&letters) {
                return Err(EngineError::at(format!("{letters} is built in and can't be redefined"), pos));
            }
            // Several letters only make sense as a function name: area(r) := ...
            if self.peek() != Some(&Tok::Sym('(')) {
                return Err(EngineError::at(
                    "Variable names are one letter, optionally with a subscript (v_{max})",
                    pos,
                ));
            }
            letters
        } else if matches!(self.toks.get(start), Some((Tok::Cmd(c), _)) if matches!(c.as_str(), "operatorname" | "mathrm" | "mathit" | "text"))
        {
            self.i = start + 1;
            self.braced_letters()?
        } else {
            self.i = start;
            self.plain_var()?
        };
        if is_builtin(&name) {
            return Err(EngineError::at(format!("{name} is built in and can't be redefined"), pos));
        }
        let mut params = Vec::new();
        if self.eat_sym('(') {
            if name.contains('_') {
                return Err(EngineError::at("Function names can't have subscripts", pos));
            }
            loop {
                let p_pos = self.pos();
                let p = self.plain_var()?;
                if is_builtin(&p) {
                    return Err(EngineError::at(format!("{p} is built in and can't be a parameter"), p_pos));
                }
                if params.contains(&p) {
                    return Err(EngineError::at(format!("{p} appears twice"), p_pos));
                }
                params.push(p);
                if self.eat_sym(')') {
                    break;
                }
                self.expect_sym(',')?;
            }
        }
        if self.i < self.toks.len() {
            return Err(EngineError::at("Only a name, or a name with parameters like f(x), goes before :=", self.pos()));
        }
        Ok((name, params))
    }

    /// One variable name, as the rest of the parser spells it: `x`, `\theta`, `x_{1}`.
    fn plain_var(&mut self) -> Result<String, EngineError> {
        let pos = self.pos();
        let base = match self.peek().cloned() {
            Some(Tok::Letter(c)) => format!("{c}"),
            Some(Tok::Cmd(n)) if GREEK.contains(&n.as_str()) => format!("\\{n}"),
            _ => return Err(EngineError::at("Expected a variable name", pos)),
        };
        self.i += 1;
        Ok(match self.subscript()? {
            Some(sub) => format!("{base}_{{{sub}}}"),
            None => base,
        })
    }

    /// `name(arg, ...)` for a user-defined function, when a bracket follows.
    fn try_user_call(&mut self, name: &str) -> Result<Option<Expr>, EngineError> {
        if !self.funcs.contains(name) {
            return Ok(None);
        }
        let save = self.i;
        let order = self.primes();
        if self.peek() != Some(&Tok::Sym('(')) {
            self.i = save;
            return Ok(None);
        }
        self.i += 1;
        let mut args = vec![self.expr()?];
        while self.eat_sym(',') {
            args.push(self.expr()?);
        }
        self.expect_sym(')')?;
        Ok(Some(if order > 0 {
            Expr::UserDeriv { name: name.to_string(), order, args }
        } else {
            Expr::UserCall(name.to_string(), args)
        }))
    }

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
        // Typed d/dx (MathLive makes \frac{d}{dx}, which the fraction handles).
        if lhs == Expr::Var("d".into())
            && matches!(
                (self.peek(), self.toks.get(self.i + 1).map(|(t, _)| t), self.toks.get(self.i + 2).map(|(t, _)| t)),
                (Some(Tok::Sym('/')), Some(Tok::Letter('d')), Some(Tok::Letter(_)))
            )
        {
            self.i += 2;
            let var = self.plain_var()?;
            if self.peek().is_none() || matches!(self.peek(), Some(Tok::Sym(')' | '}' | ']' | '+' | '-'))) {
                return Err(EngineError::at("Write what to differentiate after d/dx", self.pos()));
            }
            let body = self.term()?;
            return Ok(Expr::Deriv { body: Box::new(body), var, order: 1 });
        }
        loop {
            if self.eat_infix_mod() {
                let rhs = self.unary()?;
                lhs = Expr::NumCall(NumFn::Mod, vec![lhs, rhs]);
                continue;
            }
            // r∠θ = r(cos θ + i sin θ), with θ in the angle mode like any trig argument.
            if self.eat_cmd("angle") {
                let theta = self.unary()?;
                let cis = Expr::Binary(
                    BinOp::Add,
                    Box::new(Expr::Call(Func::Cos, Box::new(theta.clone()))),
                    Box::new(Expr::Binary(
                        BinOp::Mul,
                        Box::new(Expr::Const(Const::I)),
                        Box::new(Expr::Call(Func::Sin, Box::new(theta))),
                    )),
                );
                lhs = Expr::Binary(BinOp::Mul, Box::new(lhs), Box::new(cis));
                continue;
            }
            if self.eat_sym('×') {
                let rhs = self.unary()?;
                lhs = Expr::MatCall(MatFn::Times, vec![lhs, rhs]);
                continue;
            }
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

    /// `17 \bmod 5`, `17 \mod 5`, `\operatorname{mod}`, or typed `17 mod 5`.
    fn eat_infix_mod(&mut self) -> bool {
        let tok = |k: usize| self.toks.get(self.i + k).map(|(t, _)| t);
        let letters = |at: usize| {
            matches!((tok(at), tok(at + 1), tok(at + 2)), (Some(Tok::Letter('m')), Some(Tok::Letter('o')), Some(Tok::Letter('d'))))
        };
        let skip = match tok(0) {
            Some(Tok::Cmd(c)) if c == "bmod" || c == "mod" => 1,
            Some(Tok::Cmd(c)) if matches!(c.as_str(), "operatorname" | "mathrm" | "text") => {
                if tok(1) == Some(&Tok::Sym('{')) && letters(2) && tok(5) == Some(&Tok::Sym('}')) { 6 } else { 0 }
            }
            // A bracket after it makes it the function mod(a, b).
            Some(Tok::Letter('m')) if letters(0) && !matches!(tok(3), None | Some(Tok::Sym('('))) => 3,
            _ => 0,
        };
        self.i += skip;
        skip > 0
    }

    /// Bracketed arguments that may hold `=` or comparisons: `(xy, x+y=10)`, `(a=3, C=60°)`.
    fn rich_args(&mut self, pos: usize) -> Result<Vec<Arg>, EngineError> {
        if !self.eat_sym('(') {
            return Err(EngineError::at("Put the arguments in brackets", pos));
        }
        // The matching bracket.
        let start = self.i;
        let mut depth = 0;
        let mut close = None;
        for k in start..self.toks.len() {
            match self.toks[k].0 {
                Tok::Sym('(' | '[' | '{') => depth += 1,
                Tok::Sym(')') if depth == 0 => {
                    close = Some(k);
                    break;
                }
                Tok::Sym(')' | ']' | '}') => depth -= 1,
                _ => {}
            }
        }
        let close = close.ok_or_else(|| EngineError::at("Expected ')'", pos))?;
        let inner = self.toks[start..close].to_vec();
        self.i = close + 1;
        let mut out = Vec::new();
        for piece in split_top(&inner, |t| *t == Tok::Sym(',')) {
            if piece.is_empty() {
                return Err(EngineError::at("An argument is missing", pos));
            }
            let end = piece.last().map(|t| t.1 + 1).unwrap_or(self.end);
            let sides = split_top(&piece, |t| *t == Tok::Sym('='));
            let has_rel = split_top(&piece, |t| matches!(t, Tok::Rel(_))).len() > 1;
            out.push(match sides.as_slice() {
                [_] if has_rel => Arg::Chain(chain(&piece, end, self.funcs)?),
                [_] => Arg::Expr(Parser::new(piece.clone(), end, self.funcs).whole_expr()?),
                [l, r] if !l.is_empty() && !r.is_empty() => {
                    let lhs = Parser::new(l.clone(), r[0].1, self.funcs).whole_expr()?;
                    Arg::Eq(lhs, Parser::new(r.clone(), end, self.funcs).whole_expr()?)
                }
                _ => return Err(EngineError::at("An argument can have one =", pos)),
            });
        }
        Ok(out)
    }

    /// `gcd(12, 18, 30)`: the bracketed arguments of a number function.
    fn num_call(&mut self, f: NumFn, pos: usize) -> Result<Expr, EngineError> {
        if !self.eat_sym('(') {
            return Err(EngineError::at(format!("{} needs brackets, as in {}(12, 18)", f.name(), f.name()), pos));
        }
        let mut args = vec![self.expr()?];
        while self.eat_sym(',') {
            args.push(self.expr()?);
        }
        self.expect_sym(')')?;
        Ok(Expr::NumCall(f, args))
    }

    fn starts_implicit_factor(&self) -> bool {
        match self.peek() {
            Some(Tok::Digit(_) | Tok::Dot | Tok::Letter(_)) => true,
            Some(Tok::Sym('(' | '[' | '{')) => true,
            // In units, `g\,°C` is g·°C; elsewhere a degree sign belongs to what precedes it.
            Some(Tok::Cmd(name)) if name == "degree" => self.unit_mode,
            Some(Tok::Cmd(name)) => !matches!(name.as_str(), "circ" | "placeholder" | "rangle"),
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
            Exponent::Degree => Ok(self.degree_or_temperature(base)),
            Exponent::Value(e) => {
                let p = Expr::Pow(Box::new(base), Box::new(e));
                self.postfix_ops(p)
            }
        }
    }

    /// After a degree sign: `20°C` and `68^{\circ}\mathrm{F}` are temperatures, `30°` an angle.
    fn degree_or_temperature(&mut self, e: Expr) -> Expr {
        match self.temperature_after_degree() {
            Some(unit) => Expr::Binary(BinOp::Mul, Box::new(e), Box::new(unit)),
            None => degrees(e),
        }
    }

    /// `C` or `F` (bare or in `\mathrm{}`) right after a degree sign.
    fn temperature_after_degree(&mut self) -> Option<Expr> {
        let tok = |k: usize| self.toks.get(self.i + k).map(|(t, _)| t);
        let (letter, len) = match (tok(0), tok(1), tok(2), tok(3)) {
            (Some(Tok::Letter(l @ ('C' | 'F'))), ..) => (*l, 1),
            (Some(Tok::Cmd(c)), Some(Tok::Sym('{')), Some(Tok::Letter(l @ ('C' | 'F'))), Some(Tok::Sym('}')))
                if matches!(c.as_str(), "mathrm" | "text" | "textrm" | "mathit" | "operatorname") =>
            {
                (*l, 4)
            }
            _ => return None,
        };
        self.i += len;
        Some(Expr::Unit(format!("°{letter}")))
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
                e = self.degree_or_temperature(e);
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
                // [1, 2, 3] is a vector.
                if open == '[' && self.peek() == Some(&Tok::Sym(',')) {
                    let mut items = vec![vec![e]];
                    while self.eat_sym(',') {
                        items.push(vec![self.expr()?]);
                    }
                    self.expect_sym(close)?;
                    return Ok(Expr::Matrix(items));
                }
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
            // `→ ^{\circ}F`: a temperature unit written with a raised circle.
            Some(Tok::Sym('^')) if self.unit_mode => {
                self.i += 1;
                match self.exponent()? {
                    Exponent::Degree => self
                        .temperature_after_degree()
                        .ok_or_else(|| EngineError::at("Expected C or F after the degree sign", self.pos())),
                    Exponent::Value(_) => Err(EngineError::at("Unexpected '^'", pos)),
                }
            }
            Some(Tok::Sym(c)) => Err(EngineError::at(format!("Unexpected '{c}'"), pos)),
            Some(Tok::Assign) => Err(EngineError::at("Unexpected :=", pos)),
            Some(Tok::Arrow) => Err(EngineError::at("Put a value before →", pos)),
            Some(Tok::Rel(r)) => Err(EngineError::at(format!("Unexpected {}", r.latex().trim()), pos)),
        }
    }

    /// After `|_`: either `{x=3, y=2}` (substitution) or `{a}^{b}` (F(b) − F(a)).
    fn bar(&mut self, e: Expr) -> Result<Expr, EngineError> {
        let pos = self.pos();
        let braced = self.eat_sym('{');
        let binding = matches!(
            (self.toks.get(self.i).map(|(t, _)| t), self.toks.get(self.i + 1).map(|(t, _)| t)),
            (Some(Tok::Letter(_) | Tok::Cmd(_)), Some(Tok::Sym('=' | '_')))
        );
        if braced && binding {
            let mut binds = Vec::new();
            loop {
                let var = self.plain_var()?;
                self.expect_sym('=')?;
                binds.push((var, self.expr()?));
                if self.eat_sym('}') {
                    break;
                }
                self.expect_sym(',')?;
            }
            return Ok(Expr::Subst(Box::new(e), binds));
        }
        let lo = if braced {
            let lo = self.expr()?;
            self.expect_sym('}')?;
            lo
        } else {
            self.single_token_atom()?
        };
        if !self.eat_sym('^') {
            return Err(EngineError::at("Write |_{x=3} to substitute, or |_{a}^{b} for F(b) − F(a)", pos));
        }
        let hi = self.group_arg()?;
        Ok(Expr::BarEval(Box::new(e), Box::new(lo), Box::new(hi)))
    }

    /// `\int f\,dx` or `\int_a^b f\,dx`, after the `\int`.
    fn integral(&mut self, pos: usize) -> Result<Expr, EngineError> {
        let (mut lo, mut hi) = (None, None);
        for _ in 0..2 {
            // MathLive's int shortcut gives \int_{#?}^{#?}: empty boxes mean no bounds.
            if self.eat_sym('_') {
                lo = self.bound()?;
            } else if self.eat_sym('^') {
                hi = self.bound()?;
            }
        }
        let bounds = match (lo, hi) {
            (Some(a), Some(b)) => Some((Box::new(a), Box::new(b))),
            (None, None) => None,
            _ => return Err(EngineError::at("An integral needs both bounds, or neither", pos)),
        };
        let d = self
            .find_differential()
            .ok_or_else(|| EngineError::at("Write what to integrate over at the end, as in \\int x\\,dx", pos))?;
        let body = if d == self.i {
            // ∫ dx = ∫ 1 dx
            Expr::Number("1".into())
        } else {
            let toks = self.toks[self.i..d].to_vec();
            Parser::new(toks, self.toks[d].1, self.funcs).whole_expr()?
        };
        // Skip the d (a letter, or \mathrm{d}), then read the variable.
        self.i = d + if matches!(self.toks[d].0, Tok::Letter('d')) { 1 } else { 4 };
        let var = self.plain_var()?;
        Ok(Expr::Integral { body: Box::new(body), var, bounds })
    }

    /// An integral bound, or None for an empty one: `{}` or `{\placeholder{}}`.
    fn bound(&mut self) -> Result<Option<Expr>, EngineError> {
        let tok = |p: &Self, k: usize| p.toks.get(p.i + k).map(|(t, _)| t.clone());
        if tok(self, 0) == Some(Tok::Sym('{')) {
            if tok(self, 1) == Some(Tok::Sym('}')) {
                self.i += 2;
                return Ok(None);
            }
            let placeholder = tok(self, 1) == Some(Tok::Cmd("placeholder".into()));
            if placeholder {
                // \placeholder{} or \placeholder{name}
                let mut k = 2;
                if tok(self, k) == Some(Tok::Sym('{')) {
                    while tok(self, k).is_some_and(|t| t != Tok::Sym('}')) {
                        k += 1;
                    }
                    k += 1;
                }
                if tok(self, k) == Some(Tok::Sym('}')) {
                    self.i += k + 1;
                    return Ok(None);
                }
            }
        }
        Ok(Some(self.with_inf(Self::group_arg)?))
    }

    /// Where the `dx` that closes the integral starts: the first `d` followed by a variable at
    /// this bracket depth, skipping the differentials of integrals nested inside.
    fn find_differential(&self) -> Option<usize> {
        let mut depth = 0i32;
        let mut pending = 1;
        let mut k = self.i;
        while k < self.toks.len() {
            let tok = |j: usize| self.toks.get(j).map(|(t, _)| t);
            match &self.toks[k].0 {
                Tok::Sym('(' | '[' | '{') => depth += 1,
                Tok::Sym(')' | ']' | '}') => {
                    depth -= 1;
                    if depth < 0 {
                        return None;
                    }
                }
                Tok::Cmd(c) if c == "int" && depth == 0 => pending += 1,
                Tok::Letter('d') if depth == 0 => {
                    let starts_var = matches!(tok(k + 1), Some(Tok::Letter(_)))
                        || matches!(tok(k + 1), Some(Tok::Cmd(c)) if GREEK.contains(&c.as_str()));
                    if starts_var {
                        pending -= 1;
                        if pending == 0 {
                            return Some(k);
                        }
                    }
                }
                Tok::Cmd(c) if depth == 0 && matches!(c.as_str(), "mathrm" | "text" | "operatorname") => {
                    let upright_d = matches!(
                        (tok(k + 1), tok(k + 2), tok(k + 3)),
                        (Some(Tok::Sym('{')), Some(Tok::Letter('d')), Some(Tok::Sym('}')))
                    );
                    if upright_d {
                        pending -= 1;
                        if pending == 0 {
                            return Some(k);
                        }
                        k += 3;
                    }
                }
                _ => {}
            }
            k += 1;
        }
        None
    }

    /// Prime marks after a function name: `f'`, `f''`, `f^{\prime}`. Returns how many.
    fn primes(&mut self) -> u32 {
        let mut count = 0;
        loop {
            if self.eat_sym('\'') {
                count += 1;
                continue;
            }
            // ^{\prime\prime} or ^\prime
            let save = self.i;
            if self.eat_sym('^') {
                let braced = self.eat_sym('{');
                let mut inner = 0;
                while self.eat_cmd("prime") {
                    inner += 1;
                }
                if inner > 0 && (!braced || self.eat_sym('}')) {
                    count += inner;
                    continue;
                }
                self.i = save;
            }
            return count;
        }
    }

    /// A run of letters read as units, splitting it greedily: `kmh` is km·h, `Nm` is N·m.
    fn unit_run(&self, word: &str, pos: usize) -> Result<Expr, EngineError> {
        let mut rest = word;
        let mut out: Option<Expr> = None;
        while !rest.is_empty() {
            let (len, unit) = (1..=rest.len())
                .rev()
                .filter(|l| rest.is_char_boundary(*l))
                .find_map(|l| units::canonical(&rest[..l]).map(|u| (l, u)))
                .ok_or_else(|| EngineError::at(format!("Unknown unit '{rest}'"), pos))?;
            let u = Expr::Unit(unit);
            out = Some(match out {
                None => u,
                Some(prev) => Expr::Binary(BinOp::Mul, Box::new(prev), Box::new(u)),
            });
            rest = &rest[len..];
        }
        out.ok_or_else(|| EngineError::at("Expected a unit", pos))
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
        if self.unit_mode {
            // `k\Omega`: the ohm sign is a command, so join it to the prefix.
            if self.eat_cmd("Omega") {
                letters.push('Ω');
            }
            return self.unit_run(&letters, pos);
        }
        // The unknown of a differential equation: y, y'', y(0), y'(0).
        let first = letters.chars().next().map(|c| c.to_string()).unwrap_or_default();
        // y_0 is a plain variable, not the unknown function.
        let subscripted = self.toks.get(start + 1).map(|(t, _)| t) == Some(&Tok::Sym('_'));
        if self.funcs.contains(&format!("'{first}")) && !subscripted {
            self.i = start + 1;
            let order = self.primes();
            let at = if self.eat_sym('(') {
                let a = self.expr()?;
                self.expect_sym(')')?;
                Some(Box::new(a))
            } else {
                None
            };
            return Ok(Expr::OdeFn { name: first, order, at, var: None });
        }
        if let Some(call) = self.try_user_call(&letters)? {
            return Ok(call);
        }
        // Longest known name at the start of the run; the rest is parsed afterwards.
        for len in (2..=letters.len()).rev() {
            if !letters.is_char_boundary(len) {
                continue;
            }
            let name = &letters[..len];
            let bracket = self.toks.get(start + len).map(|(t, _)| t) == Some(&Tok::Sym('('));
            // Typed lim_{x\to0}, sum_{k=1}^{n}, prod_…, infty (in a bound).
            let subscript = self.toks.get(start + len).map(|(t, _)| t) == Some(&Tok::Sym('_'));
            if subscript && matches!(name, "lim" | "sum" | "prod") {
                self.i = start + len;
                return self.command(name, pos);
            }
            if name == "infty" && self.allow_inf {
                self.i = start + len;
                return Ok(Expr::Const(Const::Inf));
            }
            // Typed Re and Im need a bracket, so R·e and I·m stay products.
            let needs_bracket = matches!(name, "Re" | "Im");
            if name == "pi" || (Func::from_name(name).is_some() && (bracket || !needs_bracket)) || SymOp::from_name(name).is_some() {
                self.i = start + len;
                return self.named(name, pos);
            }
            // Number functions only with a bracket, so a product like n·C·r stays one.
            if bracket_name(name) && bracket {
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
                // \frac{dy}{dx}, \frac{d^2y}{dx^2}: a derivative of the unknown function.
                if let Expr::Binary(BinOp::Mul, d, f) = &num
                    && let Expr::OdeFn { name, order: 0, at: None, .. } = &**f
                    && let Some((var, order)) = derivative_operator(d, &den)
                {
                    return Ok(Expr::OdeFn { name: name.clone(), order, at: None, var: Some(var) });
                }
                if let Some((var, order)) = derivative_operator(&num, &den) {
                    // d/dx applies to the rest of the product: d/dx x² + 1 is (x²)′ + 1.
                    if self.peek().is_none() || matches!(self.peek(), Some(Tok::Sym(')' | '}' | ']' | '+' | '-'))) {
                        return Err(EngineError::at("Write what to differentiate after d/dx", self.pos()));
                    }
                    let body = self.term()?;
                    return Ok(Expr::Deriv { body: Box::new(body), var, order });
                }
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
            "operatorname" | "mathrm" | "mathit" | "text" | "textrm" | "mathop" => {
                let save = self.i;
                let word = match self.braced_letters() {
                    Ok(w) => w,
                    Err(e) => {
                        // Not one word: maybe units such as \mathrm{km/h}.
                        self.i = save;
                        return self.in_unit_mode(Self::braced).map_err(|_| e);
                    }
                };
                if self.unit_mode {
                    return self.unit_run(&word, pos);
                }
                if let Some(call) = self.try_user_call(&word)? {
                    return Ok(call);
                }
                // Upright letters that aren't a function name are a unit: \mathrm{kg}.
                let function = Func::from_name(&word).is_some() || SymOp::from_name(&word).is_some() || NumFn::from_name(&word).is_some() || DivPart::from_name(&word).is_some() || MatFn::from_name(&word).is_some() || ((StatFn::from_name(&word).is_some() || GeoFn::from_name(&word).is_some() || goal_of(&word).is_some()) && self.peek() == Some(&Tok::Sym('('))) || matches!(word.as_str(), "taylor" | "series");
                if !function && let Some(unit) = units::canonical(&word) {
                    return Ok(Expr::Unit(unit));
                }
                // Several units in one group: \text{N m}, \mathrm{kWh}.
                if !function && let Ok(units) = self.unit_run(&word, pos) {
                    return Ok(units);
                }
                self.named(&word, pos)
            }
            "int" => self.integral(pos),
            "overline" => Ok(Expr::Call(Func::Conj, Box::new(self.group_arg()?))),
            // From flatten_environments: \pmatrix{1, 2; 3, 4}
            "pmatrix" | "bmatrix" | "Bmatrix" | "matrix" | "smallmatrix" | "vmatrix" | "Vmatrix" => {
                self.expect_sym('{')?;
                let mut rows = vec![vec![]];
                if self.peek() != Some(&Tok::Sym('}')) {
                    loop {
                        rows.last_mut().expect("a row").push(self.expr()?);
                        if self.eat_sym(',') {
                            continue;
                        }
                        if self.eat_sym(';') {
                            // A trailing \\ before \end leaves an empty last row.
                            if self.peek() == Some(&Tok::Sym('}')) {
                                break;
                            }
                            rows.push(vec![]);
                            continue;
                        }
                        break;
                    }
                }
                self.expect_sym('}')?;
                let width = rows[0].len();
                if width == 0 || rows.iter().any(|r| r.len() != width) {
                    return Err(EngineError::at("Every row of a matrix needs the same number of entries", pos));
                }
                let m = Expr::Matrix(rows);
                Ok(if name.starts_with('v') || name.starts_with('V') { Expr::MatCall(MatFn::Det, vec![m]) } else { m })
            }
            "langle" => {
                let mut items = vec![vec![self.expr()?]];
                while self.eat_sym(',') {
                    items.push(vec![self.expr()?]);
                }
                if !self.eat_cmd("rangle") {
                    return Err(EngineError::at("Close the vector with ⟩", self.pos()));
                }
                Ok(Expr::Matrix(items))
            }
            "binom" | "dbinom" | "tbinom" => {
                let n = self.group_arg()?;
                let k = self.group_arg()?;
                Ok(Expr::NumCall(NumFn::Binom, vec![n, k]))
            }
            "celsius" => Ok(Expr::Unit("°C".into())),
            "fahrenheit" => Ok(Expr::Unit("°F".into())),
            "degree" => self
                .temperature_after_degree()
                .ok_or_else(|| EngineError::at("A degree sign needs a number before it", pos)),
            "Omega" if self.unit_mode => self.unit_run("Ω", pos),
            "mu" if self.unit_mode => {
                let mut letters = String::from("µ");
                while let Some(Tok::Letter(c)) = self.peek() {
                    letters.push(*c);
                    self.i += 1;
                }
                if self.eat_cmd("Omega") {
                    letters.push('Ω');
                }
                self.unit_run(&letters, pos)
            }
            "placeholder" => Err(EngineError::at("Fill in the empty box first", pos)),
            "infty" if self.allow_inf => Ok(Expr::Const(Const::Inf)),
            "infty" => Err(EngineError::at("Infinity isn't a number you can calculate with", pos)),
            "lim" => self.limit(pos),
            "sum" => self.sum(false, pos),
            "prod" => self.sum(true, pos),
            _ => match self.try_user_call(&format!("\\{name}"))? {
                Some(call) => Ok(call),
                None => self.named(name, pos),
            },
        }
    }

    fn braced_letters(&mut self) -> Result<String, EngineError> {
        self.expect_sym('{')?;
        let mut s = String::new();
        loop {
            match self.peek() {
                Some(Tok::Letter(c)) => s.push(*c),
                // Unit spellings: \mathrm{k\Omega}, \mathrm{\mu m}, \mathrm{°C}
                Some(Tok::Cmd(c)) if c == "Omega" => s.push('Ω'),
                Some(Tok::Cmd(c)) if c == "mu" => s.push('µ'),
                Some(Tok::Cmd(c)) if c == "degree" => s.push('°'),
                _ => break,
            }
            self.i += 1;
        }
        self.expect_sym('}')?;
        Ok(s)
    }

    fn named(&mut self, name: &str, pos: usize) -> Result<Expr, EngineError> {
        // With a subscript, e and i are ordinary variables: e_1, i_{max}.
        let subscripted = self.peek() == Some(&Tok::Sym('_'));
        match name {
            "pi" => return Ok(Expr::Const(Const::Pi)),
            "e" if !subscripted => return Ok(Expr::Const(Const::E)),
            "exponentialE" => return Ok(Expr::Const(Const::E)),
            "i" if !subscripted => return Ok(Expr::Const(Const::I)),
            "imaginaryI" | "imaginaryJ" => return Ok(Expr::Const(Const::I)),
            _ => {}
        }
        // Γ is a Greek letter unless a bracket makes it the gamma function.
        let letter_gamma = name == "Gamma" && self.peek() != Some(&Tok::Sym('('));
        if let Some(f) = Func::from_name(name).filter(|_| !letter_gamma) {
            return self.call(f);
        }
        if matches!(name, "taylor" | "series") {
            return self.taylor(pos);
        }
        if let Some(goal) = goal_of(name) {
            return Ok(Expr::Optimize(goal, self.rich_args(pos)?));
        }
        if let Some(f) = GeoFn::from_name(name)
            && self.peek() == Some(&Tok::Sym('('))
        {
            return Ok(Expr::Geometry(f, self.rich_args(pos)?));
        }
        if let Some(f) = StatFn::from_name(name)
            && self.peek() == Some(&Tok::Sym('('))
        {
            let Expr::NumCall(_, args) = self.num_call(NumFn::Gcd, pos)? else { unreachable!("num_call makes a NumCall") };
            return Ok(Expr::StatCall(f, name.to_string(), args));
        }
        if let Some(f) = MatFn::from_name(name) {
            // det(A), \det A, cross(u, v), identity(3)
            if self.peek() == Some(&Tok::Sym('(')) {
                let Expr::NumCall(_, args) = self.num_call(NumFn::Gcd, pos)? else { unreachable!("num_call makes a NumCall") };
                return Ok(Expr::MatCall(f, args));
            }
            return Ok(Expr::MatCall(f, vec![self.power()?]));
        }
        if let Some(part) = DivPart::from_name(name) {
            let Expr::NumCall(_, mut args) = self.num_call(NumFn::Gcd, pos).map_err(|_| EngineError::at(format!("Write {name}(p, d)"), pos))? else {
                unreachable!("num_call makes a NumCall")
            };
            if args.len() != 2 {
                return Err(EngineError::at(format!("{name} takes two things: {name}(p, d)"), pos));
            }
            let bottom = Box::new(args.pop().expect("two"));
            let top = Box::new(args.pop().expect("two"));
            return Ok(Expr::PolyDiv { top, bottom, part });
        }
        if let Some(f) = NumFn::from_name(name) {
            return self.num_call(f, pos);
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
                Tok::Assign | Tok::Arrow | Tok::Rel(_) => return Err(EngineError::at("Unfinished subscript", pos)),
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

/// `d/dx`, `d²/dx²`, `\mathrm{d}/\mathrm{d}x` as a fraction: (variable, order).
fn derivative_operator(num: &Expr, den: &Expr) -> Option<(String, u32)> {
    let is_d = |e: &Expr| matches!(e, Expr::Var(v) | Expr::Unit(v) if v == "d");
    let order_of = |e: &Expr| -> Option<u32> {
        match e {
            Expr::Number(k) => k.parse().ok().filter(|k| (1..=20).contains(k)),
            _ => None,
        }
    };
    let order = match num {
        d if is_d(d) => 1,
        Expr::Pow(b, k) if is_d(b) => order_of(k)?,
        _ => return None,
    };
    let Expr::Binary(BinOp::Mul, d, x) = den else { return None };
    if !is_d(d) {
        return None;
    }
    let (var, den_order) = match &**x {
        Expr::Var(v) => (v.clone(), 1),
        Expr::Pow(b, k) => match &**b {
            Expr::Var(v) => (v.clone(), order_of(k)?),
            _ => return None,
        },
        _ => return None,
    };
    (den_order == order).then_some((var, order))
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
