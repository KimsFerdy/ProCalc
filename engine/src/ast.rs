//! Expression tree produced by the parser.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Const {
    Pi,
    E,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Asinh,
    Acosh,
    Atanh,
    Ln,
    /// Base-10 logarithm unless a base is given with `\log_{b}`.
    Log,
    Exp,
    Sqrt,
    Abs,
    Floor,
    Ceil,
}

impl Func {
    /// Names accepted both as LaTeX commands (`\sin`) and as plain words (`sin`).
    pub fn from_name(name: &str) -> Option<Func> {
        Some(match name {
            "sin" => Func::Sin,
            "cos" => Func::Cos,
            "tan" => Func::Tan,
            "arcsin" | "asin" => Func::Asin,
            "arccos" | "acos" => Func::Acos,
            "arctan" | "atan" => Func::Atan,
            "sinh" => Func::Sinh,
            "cosh" => Func::Cosh,
            "tanh" => Func::Tanh,
            "arsinh" | "asinh" | "arcsinh" => Func::Asinh,
            "arcosh" | "acosh" | "arccosh" => Func::Acosh,
            "artanh" | "atanh" | "arctanh" => Func::Atanh,
            "ln" => Func::Ln,
            "log" | "lg" => Func::Log,
            "exp" => Func::Exp,
            "sqrt" => Func::Sqrt,
            "abs" => Func::Abs,
            "floor" => Func::Floor,
            "ceil" => Func::Ceil,
            _ => return None,
        })
    }

    pub fn latex(self) -> &'static str {
        match self {
            Func::Sin => "\\sin",
            Func::Cos => "\\cos",
            Func::Tan => "\\tan",
            Func::Asin => "\\arcsin",
            Func::Acos => "\\arccos",
            Func::Atan => "\\arctan",
            Func::Sinh => "\\sinh",
            Func::Cosh => "\\cosh",
            Func::Tanh => "\\tanh",
            Func::Asinh => "\\operatorname{arsinh}",
            Func::Acosh => "\\operatorname{arcosh}",
            Func::Atanh => "\\operatorname{artanh}",
            Func::Ln => "\\ln",
            Func::Log => "\\log",
            Func::Exp => "\\exp",
            Func::Sqrt => "\\sqrt",
            Func::Abs => "\\operatorname{abs}",
            Func::Floor => "\\operatorname{floor}",
            Func::Ceil => "\\operatorname{ceil}",
        }
    }
}

/// Operations that rewrite an expression without changing its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymOp {
    Expand,
    Factor,
}

impl SymOp {
    pub fn from_name(name: &str) -> Option<SymOp> {
        match name {
            "expand" => Some(SymOp::Expand),
            "factor" => Some(SymOp::Factor),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SymOp::Expand => "expand",
            SymOp::Factor => "factor",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Decimal literal exactly as typed, e.g. "12.50".
    Number(String),
    Const(Const),
    /// A variable, stored as its LaTeX (`x`, `\theta`, `x_{1}`).
    Var(String),
    Neg(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    /// n-th root: `\sqrt[n]{x}`.
    Root(Box<Expr>, Box<Expr>),
    /// `\log_{base}(x)`.
    LogBase(Box<Expr>, Box<Expr>),
    Call(Func, Box<Expr>),
    /// `n!` (step 1) or the double factorial `n!!` (step 2).
    Factorial(Box<Expr>, u32),
    /// x% = x/100.
    Percent(Box<Expr>),
    /// `expand(...)`, `factor(...)`.
    Op(SymOp, Box<Expr>),
}
