//! Expression tree produced by the parser.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Const {
    Pi,
    E,
    /// The imaginary unit i.
    I,
    /// ∞: only as a bound (∫₀^∞, Σ to ∞), a limit's target or a limit's value, never in
    /// arithmetic. The parser accepts `\infty` nowhere else.
    Inf,
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
    /// Γ(x), the factorial moved down one: Γ(n) = (n−1)!.
    Gamma,
    /// Real part.
    Re,
    /// Imaginary part.
    Im,
    /// The angle of a complex number, in (−π, π].
    Arg,
    /// Complex conjugate, written as an overline.
    Conj,
    /// The error function.
    Erf,
    /// The standard normal quantile Φ⁻¹(p) (`invnorm`).
    Probit,
}

/// Number functions with whole-number arguments: `gcd(12, 18)`, `\binom{5}{2}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumFn {
    Gcd,
    Lcm,
    Mod,
    Binom,
    Perm,
    Totient,
    /// Yes/no: shown on its own line.
    IsPrime,
    /// A list: shown on its own line.
    Divisors,
    /// All n-th roots, complex ones included: shown on its own line.
    Roots,
}

impl NumFn {
    pub fn from_name(name: &str) -> Option<NumFn> {
        Some(match name {
            "gcd" | "hcf" => NumFn::Gcd,
            "lcm" => NumFn::Lcm,
            "mod" => NumFn::Mod,
            "nCr" | "binom" | "choose" => NumFn::Binom,
            "nPr" => NumFn::Perm,
            "totient" => NumFn::Totient,
            "isprime" => NumFn::IsPrime,
            "divisors" => NumFn::Divisors,
            "roots" => NumFn::Roots,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            NumFn::Gcd => "gcd",
            NumFn::Lcm => "lcm",
            NumFn::Mod => "mod",
            NumFn::Binom => "nCr",
            NumFn::Perm => "nPr",
            NumFn::Totient => "totient",
            NumFn::IsPrime => "isprime",
            NumFn::Divisors => "divisors",
            NumFn::Roots => "roots",
        }
    }
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
            "Gamma" | "tgamma" => Func::Gamma,
            "Re" | "real" => Func::Re,
            "Im" | "imag" => Func::Im,
            "arg" => Func::Arg,
            "conj" => Func::Conj,
            "erf" => Func::Erf,
            "probit" => Func::Probit,
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
            Func::Gamma => "\\Gamma",
            Func::Re => "\\operatorname{Re}",
            Func::Im => "\\operatorname{Im}",
            Func::Arg => "\\arg",
            Func::Conj => "\\operatorname{conj}",
            Func::Erf => "\\operatorname{erf}",
            Func::Probit => "\\Phi^{-1}",
        }
    }
}

/// Functions of matrices and vectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatFn {
    Det,
    Inv,
    Transpose,
    Trace,
    Rank,
    Rref,
    /// A list: shown on its own line.
    Eigenvalues,
    /// A list: shown on its own line.
    Eigenvectors,
    Dot,
    Cross,
    Norm,
    /// `identity(n)`: the n×n identity matrix.
    Identity,
    /// `a \times b`: the cross product of two vectors, otherwise ordinary multiplication.
    Times,
}

impl MatFn {
    pub fn from_name(name: &str) -> Option<MatFn> {
        Some(match name {
            "det" => MatFn::Det,
            "inv" | "inverse" => MatFn::Inv,
            "transpose" => MatFn::Transpose,
            "tr" | "trace" => MatFn::Trace,
            "rank" => MatFn::Rank,
            "rref" => MatFn::Rref,
            "eigenvalues" | "eigvals" | "eig" => MatFn::Eigenvalues,
            "eigenvectors" | "eigvecs" => MatFn::Eigenvectors,
            "dot" => MatFn::Dot,
            "cross" => MatFn::Cross,
            "norm" => MatFn::Norm,
            "identity" | "eye" => MatFn::Identity,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            MatFn::Det => "det",
            MatFn::Inv => "inv",
            MatFn::Transpose => "transpose",
            MatFn::Trace => "trace",
            MatFn::Rank => "rank",
            MatFn::Rref => "rref",
            MatFn::Eigenvalues => "eigenvalues",
            MatFn::Eigenvectors => "eigenvectors",
            MatFn::Dot => "dot",
            MatFn::Cross => "cross",
            MatFn::Norm => "norm",
            MatFn::Identity => "identity",
            MatFn::Times => "×",
        }
    }
}

/// Statistics of a list and probability distributions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatFn {
    Mean,
    Median,
    Mode,
    /// Sample variance (n − 1).
    Var,
    /// Population variance (n).
    PVar,
    /// Sample standard deviation.
    Sd,
    /// Population standard deviation.
    PSd,
    Min,
    Max,
    Range,
    Total,
    Count,
    /// `quartile(L, 1)` or `quartile(L, 3)`.
    Quartile,
    Iqr,
    Percentile,
    Corr,
    Cov,
    /// A list: shown on its own line.
    LinReg,
    /// A list: shown on its own line.
    Summary,
    BinomPdf,
    BinomCdf,
    PoissonPdf,
    PoissonCdf,
    GeomPdf,
    GeomCdf,
    NormalPdf,
    NormalCdf,
    InvNorm,
}

impl StatFn {
    pub fn from_name(name: &str) -> Option<StatFn> {
        Some(match name {
            "mean" | "avg" | "average" => StatFn::Mean,
            "median" => StatFn::Median,
            "mode" => StatFn::Mode,
            "var" | "variance" => StatFn::Var,
            "pvar" => StatFn::PVar,
            "sd" | "stdev" | "std" => StatFn::Sd,
            "psd" | "pstdev" => StatFn::PSd,
            "min" => StatFn::Min,
            "max" => StatFn::Max,
            "range" => StatFn::Range,
            "sum" | "total" => StatFn::Total,
            "count" | "length" => StatFn::Count,
            "quartile" => StatFn::Quartile,
            "iqr" => StatFn::Iqr,
            "percentile" => StatFn::Percentile,
            "corr" | "correlation" => StatFn::Corr,
            "cov" | "covariance" => StatFn::Cov,
            "linreg" | "regress" => StatFn::LinReg,
            "stats" | "summary" => StatFn::Summary,
            "binompdf" => StatFn::BinomPdf,
            "binomcdf" => StatFn::BinomCdf,
            "poissonpdf" => StatFn::PoissonPdf,
            "poissoncdf" => StatFn::PoissonCdf,
            "geompdf" | "geometpdf" => StatFn::GeomPdf,
            "geomcdf" | "geometcdf" => StatFn::GeomCdf,
            "normalpdf" | "normpdf" => StatFn::NormalPdf,
            "normalcdf" | "normcdf" => StatFn::NormalCdf,
            "invnorm" | "invNorm" => StatFn::InvNorm,
            _ => return None,
        })
    }
}

/// An argument that may hold `=` or comparisons: `x+y=10`, `a=3`, `0 \le x \le 5`.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Expr(Expr),
    Eq(Expr, Expr),
    Chain(Chain),
}

/// What `minimize`, `maximize` and `extrema` look for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Goal {
    Min,
    Max,
    /// Every critical point, classified.
    All,
}

/// Geometry: shapes, triangles and coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeoFn {
    Triangle,
    Circle,
    Square,
    Rectangle,
    Trapezoid,
    Sphere,
    Cylinder,
    Cone,
    Cube,
    Box,
    Distance,
    Midpoint,
    Slope,
    Line,
    Angle,
    Polygon,
}

impl GeoFn {
    pub fn from_name(name: &str) -> Option<GeoFn> {
        Some(match name {
            "triangle" => GeoFn::Triangle,
            "circle" => GeoFn::Circle,
            "square" => GeoFn::Square,
            "rectangle" => GeoFn::Rectangle,
            "trapezoid" | "trapezium" => GeoFn::Trapezoid,
            "sphere" => GeoFn::Sphere,
            "cylinder" => GeoFn::Cylinder,
            "cone" => GeoFn::Cone,
            "cube" => GeoFn::Cube,
            "cuboid" | "prism" => GeoFn::Box,
            "distance" | "dist" => GeoFn::Distance,
            "midpoint" => GeoFn::Midpoint,
            "slope" | "gradient" => GeoFn::Slope,
            "line" => GeoFn::Line,
            "angle" => GeoFn::Angle,
            "polygon" => GeoFn::Polygon,
            _ => return None,
        })
    }
}

/// Which result of a long division is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivPart {
    /// q + r/d, shown as both on its own line.
    Both,
    Quotient,
    Remainder,
}

impl DivPart {
    pub fn from_name(name: &str) -> Option<DivPart> {
        Some(match name {
            "divide" | "polydiv" => DivPart::Both,
            "quotient" => DivPart::Quotient,
            "remainder" => DivPart::Remainder,
            _ => return None,
        })
    }
}

/// Operations that rewrite an expression without changing its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymOp {
    Expand,
    Factor,
    /// A complex number shown as r·e^{iθ}.
    Polar,
    /// One fraction with common factors cancelled.
    Simplify,
    /// Partial fractions.
    Apart,
}

impl SymOp {
    pub fn from_name(name: &str) -> Option<SymOp> {
        match name {
            "expand" => Some(SymOp::Expand),
            "factor" => Some(SymOp::Factor),
            "polar" => Some(SymOp::Polar),
            "simplify" | "cancel" | "together" => Some(SymOp::Simplify),
            "apart" | "partfrac" => Some(SymOp::Apart),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            SymOp::Expand => "expand",
            SymOp::Factor => "factor",
            SymOp::Polar => "polar",
            SymOp::Simplify => "simplify",
            SymOp::Apart => "apart",
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
    /// A call to a function the user defined with `f(x) := ...`.
    UserCall(String, Vec<Expr>),
    /// A unit, by canonical name (`km`, `°C`).
    Unit(String),
    /// `gcd(12, 18)`, `17 \bmod 5`, `\binom{5}{2}`…
    NumCall(NumFn, Vec<Expr>),
    /// `x → units`: x converted to the target units.
    Convert(Box<Expr>, Box<Expr>),
    /// `d^n/dx^n body`.
    Deriv { body: Box<Expr>, var: String, order: u32 },
    /// `f'(x)`, `f''(x)`: a derivative of a user function at the arguments.
    UserDeriv { name: String, order: u32, args: Vec<Expr> },
    /// `∫ body dx`, or `∫_a^b body dx` with bounds.
    Integral { body: Box<Expr>, var: String, bounds: Option<(Box<Expr>, Box<Expr>)> },
    /// `\lim_{x\to a} body`; `side` is −1 for a⁻, 1 for a⁺, 0 for both sides.
    Limit { body: Box<Expr>, var: String, to: Box<Expr>, side: i8 },
    /// `\sum_{k=a}^{b} body`, or `\prod` when `product`.
    Sum { body: Box<Expr>, var: String, lo: Box<Expr>, hi: Box<Expr>, product: bool },
    /// The unknown function of a differential equation: `y`, `y'`, `y''` (`at: None`), or its
    /// value in an initial condition, `y(0)`, `y'(0)`. `var` is the independent variable when
    /// the input names it (`\frac{dy}{dt}`).
    OdeFn { name: String, order: u32, at: Option<Box<Expr>>, var: Option<String> },
    /// A matrix, by rows; a vector is one column.
    Matrix(Vec<Vec<Expr>>),
    /// `mean([1,2,3])`, `binompdf(10, 0.5, 3)`, …
    StatCall(StatFn, String, Vec<Expr>),
    /// `minimize(f)`, `maximize(xy, x+y=10)`, `extrema(f, 0 \le x \le 5)`.
    Optimize(Goal, Vec<Arg>),
    /// `triangle(a=3, b=4, C=60°)`, `circle(r=2)`, `distance([1,2],[4,6])`.
    Geometry(GeoFn, Vec<Arg>),
    /// `det(A)`, `A \times B`, …
    MatCall(MatFn, Vec<Expr>),
    /// `divide(p, d)`, `quotient(p, d)`, `remainder(p, d)`: polynomial long division.
    PolyDiv { top: Box<Expr>, bottom: Box<Expr>, part: DivPart },
    /// `taylor(f, x, a, n)`: the Taylor polynomial of f about x = a up to (x−a)ⁿ.
    Taylor { body: Box<Expr>, var: Option<String>, at: Box<Expr>, order: u32 },
    /// `body|_{x=3, y=2}`: the body with the variables replaced.
    Subst(Box<Expr>, Vec<(String, Expr)>),
    /// `F|_a^b` = F(b) − F(a), for an expression with a single free variable.
    BarEval(Box<Expr>, Box<Expr>, Box<Expr>),
}

/// One line of input: a plain expression, or a definition.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Expr(Expr),
    /// `x := 5` (no params) or `f(x, y) := x + y`.
    Define { name: String, params: Vec<String>, body: Expr },
    /// One equation or a system: `2x+1=7`, `x+y=3, x-y=1`, as (left, right) pairs, with any
    /// conditions after them (`0 \le x \le 2\pi`) that restrict the solutions or the plot.
    Equations(Vec<(Expr, Expr)>, Vec<Chain>),
    /// Inequalities only: `x^2 < 4`, `y > x^2`, `x > 0, y > 0`.
    Inequalities(Vec<Chain>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rel {
    Lt,
    Le,
    Gt,
    Ge,
}

impl Rel {
    pub fn latex(self) -> &'static str {
        match self {
            Rel::Lt => "<",
            Rel::Le => "\\le ",
            Rel::Gt => ">",
            Rel::Ge => "\\ge ",
        }
    }

    pub fn strict(self) -> bool {
        matches!(self, Rel::Lt | Rel::Gt)
    }

    /// Whether `a rel b` holds, for a = lhs − rhs compared with 0.
    pub fn holds(self, diff: f64) -> bool {
        match self {
            Rel::Lt => diff < 0.0,
            Rel::Le => diff <= 0.0,
            Rel::Gt => diff > 0.0,
            Rel::Ge => diff >= 0.0,
        }
    }
}

/// A chain of comparisons: `a < b`, `0 \le t \le 4\pi` (parts.len() = rels.len() + 1).
#[derive(Debug, Clone, PartialEq)]
pub struct Chain {
    pub parts: Vec<Expr>,
    pub rels: Vec<Rel>,
}

/// How a user-defined name prints: `f`, `\phi`, `\operatorname{area}`.
pub fn name_latex(name: &str) -> String {
    if name.starts_with('\\') || name.chars().count() == 1 || name.contains('_') {
        name.to_string()
    } else {
        format!("\\operatorname{{{name}}}")
    }
}
