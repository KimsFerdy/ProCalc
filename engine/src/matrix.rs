//! Matrices and vectors: arithmetic, determinants, inverses, row reduction, eigenvalues and
//! vector products, with the working.
//!
//! Entries are symbolic, so letters and surds work. Matrix products don't commute, so they
//! are computed here when an expression is built and never handed to the canonical `mul`,
//! which sorts its factors.

use num_rational::BigRational as Q;
use num_traits::{ToPrimitive, Zero};

use crate::ast::{BinOp, Const, MatFn};
use crate::latex::latex;
use crate::sym::{self, Builder, Sym, add, div, mul, n, neg, pow};
use crate::{EngineError, Method, Step};

type R<T> = Result<T, EngineError>;
pub type M = Vec<Vec<Sym>>;

/// Largest matrix side handled.
const MAX_SIZE: usize = 12;
/// Row operations shown in the working before the rest are summarised.
const MAX_SHOWN_OPS: usize = 14;

fn step(latex: impl Into<String>, note: impl Into<String>) -> Step {
    Step { latex: latex.into(), note: note.into() }
}

/// Expanded, with complex parts tidied: entries stay in one canonical shape so zero tests work.
fn simp(s: Sym) -> Sym {
    sym::normalize_complex(crate::poly::expand(&s).unwrap_or(s))
}

fn dims(m: &M) -> (usize, usize) {
    (m.len(), m.first().map_or(0, Vec::len))
}

fn is_vector(m: &M) -> bool {
    let (r, c) = dims(m);
    r == 1 || c == 1
}

fn flat(m: &M) -> Vec<Sym> {
    m.iter().flatten().cloned().collect()
}

pub fn matrix_latex(m: &M) -> String {
    let rows: Vec<String> = m.iter().map(|r| r.iter().map(latex).collect::<Vec<_>>().join("&")).collect();
    format!("\\begin{{pmatrix}}{}\\end{{pmatrix}}", rows.join("\\\\"))
}

fn identity(k: usize) -> M {
    (0..k).map(|i| (0..k).map(|j| n((i == j) as i64)).collect()).collect()
}

fn transpose(m: &M) -> M {
    let (r, c) = dims(m);
    (0..c).map(|j| (0..r).map(|i| m[i][j].clone()).collect()).collect()
}

fn product(a: &M, b: &M) -> R<M> {
    let ((ra, ca), (rb, cb)) = (dims(a), dims(b));
    if ca != rb {
        return Err(EngineError::new(format!("Can't multiply a {ra}×{ca} matrix by a {rb}×{cb} one: the inner sizes must match")));
    }
    Ok((0..ra).map(|i| (0..cb).map(|j| simp(add((0..ca).map(|k| mul(vec![a[i][k].clone(), b[k][j].clone()])).collect()))).collect()).collect())
}

fn scale(m: &M, k: &Sym) -> M {
    m.iter().map(|r| r.iter().map(|e| simp(mul(vec![k.clone(), e.clone()]))).collect()).collect()
}

fn square(m: &M, what: &str) -> R<usize> {
    let (r, c) = dims(m);
    if r != c {
        return Err(EngineError::new(format!("{what} needs a square matrix, not {r}×{c}")));
    }
    if r > MAX_SIZE {
        return Err(EngineError::new(format!("Matrices are limited to {MAX_SIZE}×{MAX_SIZE}")));
    }
    Ok(r)
}

fn rational(m: &M) -> bool {
    m.iter().flatten().all(|e| e.as_num().is_some())
}

/// R_{i}
fn row(i: usize) -> String {
    format!("R_{{{}}}", i + 1)
}

/// Row reduction to reduced row echelon form: (result, pivot columns, operations shown).
fn rref(mut m: M) -> (M, Vec<usize>, Vec<Step>) {
    let (rows, cols) = dims(&m);
    let mut pivots = Vec::new();
    let mut ops = Vec::new();
    let mut lead = 0;
    let note = |ops: &mut Vec<Step>, op: String, m: &M| {
        if ops.len() < MAX_SHOWN_OPS {
            ops.push(step(matrix_latex(m), op));
        }
    };
    for col in 0..cols {
        if lead >= rows {
            break;
        }
        let Some(p) = (lead..rows).find(|&r| !m[r][col].is_zero()) else { continue };
        if p != lead {
            m.swap(p, lead);
            note(&mut ops, format!("${}\\leftrightarrow {}$", row(lead), row(p)), &m);
        }
        let pivot = m[lead][col].clone();
        if !pivot.is_one() {
            let inv = div(n(1), pivot.clone()).map(simp).unwrap_or_else(|_| n(1));
            m[lead] = m[lead].iter().map(|e| simp(mul(vec![inv.clone(), e.clone()]))).collect();
            note(&mut ops, format!("${}\\to {}{}$", row(lead), coeff_latex(&inv), row(lead)), &m);
        }
        for r in 0..rows {
            if r == lead || m[r][col].is_zero() {
                continue;
            }
            let f = m[r][col].clone();
            let base = m[lead].clone();
            m[r] = m[r].iter().zip(&base).map(|(e, b)| simp(add(vec![e.clone(), neg(mul(vec![f.clone(), b.clone()]))]))).collect();
            note(&mut ops, combine(r, &f, lead), &m);
        }
        pivots.push(col);
        lead += 1;
    }
    (m, pivots, ops)
}

/// `R_{3}\to R_{3}-2R_{1}`, or `+\frac{1}{2}R_{1}` for a negative multiplier.
fn combine(target: usize, f: &Sym, source: usize) -> String {
    let (c, rest) = sym::split_coeff(f.clone());
    if c < Q::zero() {
        let positive = mul(vec![Sym::Num(-c), rest]);
        format!("${}\\to {}+{}{}$", row(target), row(target), coeff_latex(&positive), row(source))
    } else {
        format!("${}\\to {}-{}{}$", row(target), row(target), coeff_latex(f), row(source))
    }
}

/// `3`, `\frac{1}{2}`, `(x+1)` as a row multiplier.
fn coeff_latex(k: &Sym) -> String {
    match k {
        k if k.is_one() => String::new(),
        Sym::Add(_) => format!("\\left({}\\right)", latex(k)),
        _ => latex(k),
    }
}

/// The minor of m without row i and column j.
fn minor(m: &M, i: usize, j: usize) -> M {
    m.iter().enumerate().filter(|(r, _)| *r != i).map(|(_, row)| row.iter().enumerate().filter(|(c, _)| *c != j).map(|(_, e)| e.clone()).collect()).collect()
}

/// det by cofactor expansion along the first row.
pub(crate) fn det_cofactor(m: &M) -> Sym {
    let k = m.len();
    match k {
        0 => n(1),
        1 => m[0][0].clone(),
        2 => simp(add(vec![mul(vec![m[0][0].clone(), m[1][1].clone()]), neg(mul(vec![m[0][1].clone(), m[1][0].clone()]))])),
        _ => simp(add(
            (0..k)
                .filter(|&j| !m[0][j].is_zero())
                .map(|j| {
                    let sign = if j % 2 == 0 { n(1) } else { n(-1) };
                    mul(vec![sign, m[0][j].clone(), det_cofactor(&minor(m, 0, j))])
                })
                .collect(),
        )),
    }
}

/// det by elimination: the product of the pivots, with a sign for each swap.
fn det_elimination(mut m: M, steps: &mut Vec<Step>) -> Sym {
    let k = m.len();
    let mut sign = 1i64;
    let mut diag = Vec::new();
    for col in 0..k {
        let Some(p) = (col..k).find(|&r| !m[r][col].is_zero()) else {
            steps.push(step(matrix_latex(&m), "A column with no pivot: the determinant is 0"));
            return n(0);
        };
        if p != col {
            m.swap(p, col);
            sign = -sign;
            if steps.len() < MAX_SHOWN_OPS {
                steps.push(step(matrix_latex(&m), format!("${}\\leftrightarrow {}$ (a swap flips the sign)", row(col), row(p))));
            }
        }
        let pivot = m[col][col].clone();
        for r in col + 1..k {
            if m[r][col].is_zero() {
                continue;
            }
            let f = simp(div(m[r][col].clone(), pivot.clone()).unwrap_or_else(|_| n(0)));
            let base = m[col].clone();
            m[r] = m[r].iter().zip(&base).map(|(e, b)| simp(add(vec![e.clone(), neg(mul(vec![f.clone(), b.clone()]))]))).collect();
            if steps.len() < MAX_SHOWN_OPS {
                steps.push(step(matrix_latex(&m), combine(r, &f, col)));
            }
        }
        diag.push(pivot);
    }
    let shown = diag.iter().map(|d| if matches!(d, Sym::Num(v) if v.is_integer() && v >= &Q::zero()) { latex(d) } else { format!("\\left({}\\right)", latex(d)) }).collect::<Vec<_>>().join("\\cdot ");
    let value = simp(mul(std::iter::once(n(sign)).chain(diag).collect()));
    steps.push(step(
        format!("{}{shown}={}", if sign < 0 { "-" } else { "" }, latex(&value)),
        "Triangular now: the determinant is the product of the diagonal",
    ));
    value
}

/// Coefficients c₀ … cₙ of det(λI − A) by Faddeev–LeVerrier (no division by entries).
fn characteristic(a: &M) -> R<Vec<Sym>> {
    let k = a.len();
    let mut c = vec![n(0); k + 1];
    c[k] = n(1);
    let mut m: M = vec![vec![n(0); k]; k];
    for step_k in 1..=k {
        // M_k = A·M_{k−1} + c_{n−k+1}·I
        let mut next = product(a, &m)?;
        for (i, r) in next.iter_mut().enumerate() {
            r[i] = simp(add(vec![r[i].clone(), c[k - step_k + 1].clone()]));
        }
        m = next;
        let am = product(a, &m)?;
        let trace = add((0..k).map(|i| am[i][i].clone()).collect());
        c[k - step_k] = simp(div(neg(trace), n(step_k as i64))?);
    }
    Ok(c)
}

impl Builder<'_> {
    /// A + B, A − B, AB, kA, A/k, and the dot product u·v of two vectors.
    pub(crate) fn matrix_binary(&mut self, op: BinOp, a: Sym, b: Sym) -> R<Sym> {
        use Sym::Matrix as Mx;
        Ok(match (op, a, b) {
            (BinOp::Add | BinOp::Sub, Mx(x), Mx(y)) => {
                if dims(&x) != dims(&y) {
                    let ((r1, c1), (r2, c2)) = (dims(&x), dims(&y));
                    return Err(EngineError::new(format!("Can't add a {r1}×{c1} matrix to a {r2}×{c2} one")));
                }
                let sub = op == BinOp::Sub;
                Mx(x.iter().zip(&y).map(|(r, s)| r.iter().zip(s).map(|(e, f)| simp(add(vec![e.clone(), if sub { neg(f.clone()) } else { f.clone() }]))).collect()).collect())
            }
            (BinOp::Add | BinOp::Sub, _, _) => return Err(EngineError::new("A matrix and a number can't be added")),
            (BinOp::Mul, Mx(x), Mx(y)) => {
                // u·v for two vectors of the same length (as columns or rows).
                if is_vector(&x) && is_vector(&y) && dims(&x) == dims(&y) && dims(&x).1 == 1 {
                    return self.dot(&x, &y);
                }
                Mx(product(&x, &y)?)
            }
            (BinOp::Mul, Mx(x), k) | (BinOp::Mul, k, Mx(x)) => Mx(scale(&x, &k)),
            (BinOp::Div, Mx(x), k) if !matches!(k, Mx(_)) => Mx(scale(&x, &div(n(1), k)?)),
            (BinOp::Div, _, _) => return Err(EngineError::new("Dividing by a matrix isn't defined: multiply by its inverse, A^{-1}")),
            _ => return Err(EngineError::new("That isn't defined for matrices")),
        })
    }

    /// −A
    pub(crate) fn matrix_neg(&mut self, m: &M) -> Sym {
        Sym::Matrix(scale(m, &n(-1)))
    }

    /// Aᵏ for a whole number k (A⁻¹ for k = −1); Aᵀ is handled by the caller.
    pub(crate) fn matrix_pow(&mut self, m: &M, e: &Sym) -> R<Sym> {
        let k = e.as_num().filter(|k| k.is_integer()).and_then(|k| k.to_integer().to_i64()).ok_or_else(|| EngineError::new("A matrix power must be a whole number (or T for the transpose)"))?;
        let size = square(m, "A power")?;
        if k.unsigned_abs() > 1000 {
            return Err(EngineError::new("That matrix power is too large"));
        }
        let base = if k < 0 { self.inverse(m)? } else { m.clone() };
        // Repeated squaring.
        let mut result = identity(size);
        let mut sq = base;
        let mut left = k.unsigned_abs();
        while left > 0 {
            if left & 1 == 1 {
                result = product(&result, &sq)?;
            }
            left >>= 1;
            if left > 0 {
                sq = product(&sq, &sq)?;
            }
        }
        Ok(Sym::Matrix(result))
    }

    fn dot(&mut self, u: &M, v: &M) -> R<Sym> {
        let (a, b) = (flat(u), flat(v));
        if a.len() != b.len() {
            return Err(EngineError::new("The dot product needs two vectors of the same length"));
        }
        let terms: Vec<Sym> = a.iter().zip(&b).map(|(x, y)| mul(vec![x.clone(), y.clone()])).collect();
        let value = simp(add(terms.clone()));
        let shown = a.iter().zip(&b).map(|(x, y)| format!("{}\\cdot {}", wrap(x), wrap(y))).collect::<Vec<_>>().join("+");
        self.ev.work.push(Method { name: "Dot product".into(), steps: vec![step(format!("{shown}={}", latex(&value)), "Multiply matching entries and add")] });
        Ok(value)
    }

    fn cross(&mut self, u: &M, v: &M) -> R<Sym> {
        let (a, b) = (flat(u), flat(v));
        if a.len() != 3 || b.len() != 3 || !is_vector(u) || !is_vector(v) {
            return Err(EngineError::new("The cross product needs two vectors with 3 entries"));
        }
        let comp = |i: usize, j: usize| simp(add(vec![mul(vec![a[i].clone(), b[j].clone()]), neg(mul(vec![a[j].clone(), b[i].clone()]))]));
        let c = vec![vec![comp(1, 2)], vec![comp(2, 0)], vec![comp(0, 1)]];
        let result = Sym::Matrix(c.clone());
        self.ev.work.push(Method {
            name: "Cross product".into(),
            steps: vec![step(
                format!("\\begin{{pmatrix}}a_2b_3-a_3b_2\\\\a_3b_1-a_1b_3\\\\a_1b_2-a_2b_1\\end{{pmatrix}}={}", matrix_latex(&c)),
                "Each entry from the other two of $u$ and $v$",
            )],
        });
        Ok(result)
    }

    /// det(A), with the working: ad − bc, cofactors, or elimination.
    pub(crate) fn det(&mut self, m: &M) -> R<Sym> {
        let k = square(m, "A determinant")?;
        let mut steps = vec![step(format!("\\det{}", matrix_latex(m)), "The determinant")];
        let value = match k {
            1 => m[0][0].clone(),
            2 => {
                let v = det_cofactor(m);
                steps.push(step(
                    format!("{}\\cdot {}-{}\\cdot {}={}", wrap(&m[0][0]), wrap(&m[1][1]), wrap(&m[0][1]), wrap(&m[1][0]), latex(&v)),
                    "$ad-bc$",
                ));
                v
            }
            3 => {
                let parts: Vec<String> = (0..3)
                    .map(|j| {
                        let sign = if j == 1 { "-" } else { "+" };
                        format!("{sign}{}\\det{}", wrap(&m[0][j]), matrix_latex(&minor(m, 0, j)))
                    })
                    .collect();
                let v = det_cofactor(m);
                steps.push(step(format!("{}={}", parts.join("").trim_start_matches('+'), latex(&v)), "Expand along the first row: signs $+,-,+$ and each entry's minor"));
                v
            }
            _ if rational(m) => det_elimination(m.clone(), &mut steps),
            _ if k <= 5 => {
                let v = det_cofactor(m);
                steps.push(step(latex(&v), "Expand by cofactors"));
                v
            }
            _ => {
                let c = characteristic(m)?;
                let v = simp(if k % 2 == 0 { c[0].clone() } else { neg(c[0].clone()) });
                steps.push(step(latex(&v), "From the characteristic polynomial (Faddeev–LeVerrier)"));
                v
            }
        };
        self.ev.work.push(Method { name: "Determinant".into(), steps });
        Ok(value)
    }

    /// A⁻¹, by the 2×2 formula or Gauss–Jordan on [A | I].
    pub(crate) fn inverse(&mut self, m: &M) -> R<M> {
        let k = square(m, "An inverse")?;
        let singular = || EngineError::new("This matrix has no inverse: its determinant is 0");
        let mut steps = Vec::new();
        let result = if k == 2 {
            let d = det_cofactor(m);
            if d.is_zero() {
                return Err(singular());
            }
            let adj = vec![vec![m[1][1].clone(), neg(m[0][1].clone())], vec![neg(m[1][0].clone()), m[0][0].clone()]];
            let inv = scale(&adj, &div(n(1), d.clone())?);
            steps.push(step(
                format!("\\frac{{1}}{{{}}}{}={}", latex(&d), matrix_latex(&adj.iter().map(|r| r.iter().map(|e| simp(e.clone())).collect()).collect()), matrix_latex(&inv)),
                "$\\frac{1}{ad-bc}$ times $a$ and $d$ swapped, $b$ and $c$ negated",
            ));
            inv
        } else {
            let id = identity(k);
            let augmented: M = m.iter().zip(&id).map(|(r, i)| r.iter().chain(i).cloned().collect()).collect();
            steps.push(step(matrix_latex(&augmented), "Put the identity beside it: $[A\\,|\\,I]$"));
            let (reduced, pivots, ops) = rref(augmented);
            if pivots.len() < k || pivots.iter().any(|&p| p >= k) {
                return Err(singular());
            }
            let shown = ops.len();
            steps.extend(ops);
            if shown >= MAX_SHOWN_OPS {
                steps.push(step("", "…and so on until the left side is the identity"));
            }
            let inv: M = reduced.iter().map(|r| r[k..].to_vec()).collect();
            steps.push(step(matrix_latex(&inv), "The right half is the inverse"));
            inv
        };
        self.ev.work.push(Method { name: "Inverse".into(), steps });
        Ok(result)
    }

    /// The value of a matrix function.
    pub(crate) fn mat_call(&mut self, f: MatFn, args: Vec<Sym>) -> R<Sym> {
        let need = |want: usize| -> R<()> {
            if args.len() == want {
                Ok(())
            } else {
                Err(EngineError::new(format!("{} takes {want} argument{}", f.name(), if want == 1 { "" } else { "s" })))
            }
        };
        let matrix = |s: &Sym| -> R<M> {
            match s {
                Sym::Matrix(m) => Ok(m.clone()),
                _ => Err(EngineError::new(format!("{} needs a matrix or a vector", f.name()))),
            }
        };
        Ok(match f {
            MatFn::Times => {
                need(2)?;
                match (&args[0], &args[1]) {
                    (Sym::Matrix(u), Sym::Matrix(v)) if is_vector(u) && is_vector(v) && flat(u).len() == 3 && flat(v).len() == 3 => self.cross(u, v)?,
                    (Sym::Matrix(_), _) | (_, Sym::Matrix(_)) => self.matrix_binary(BinOp::Mul, args[0].clone(), args[1].clone())?,
                    _ => mul(args),
                }
            }
            MatFn::Identity => {
                need(1)?;
                let k = args[0].as_num().filter(|k| k.is_integer()).and_then(|k| k.to_integer().to_usize()).filter(|k| (1..=MAX_SIZE).contains(k));
                Sym::Matrix(identity(k.ok_or_else(|| EngineError::new(format!("identity(n) needs n from 1 to {MAX_SIZE}")))?))
            }
            MatFn::Det => {
                need(1)?;
                self.det(&matrix(&args[0])?)?
            }
            MatFn::Inv => {
                need(1)?;
                Sym::Matrix(self.inverse(&matrix(&args[0])?)?)
            }
            MatFn::Transpose => {
                need(1)?;
                Sym::Matrix(transpose(&matrix(&args[0])?))
            }
            MatFn::Trace => {
                need(1)?;
                let m = matrix(&args[0])?;
                let k = square(&m, "The trace")?;
                simp(add((0..k).map(|i| m[i][i].clone()).collect()))
            }
            MatFn::Rank | MatFn::Rref => {
                need(1)?;
                let m = matrix(&args[0])?;
                let mut steps = vec![step(matrix_latex(&m), "The matrix")];
                let (reduced, pivots, ops) = rref(m);
                let many = ops.len() >= MAX_SHOWN_OPS;
                steps.extend(ops);
                if many {
                    steps.push(step("", "…and so on"));
                }
                let result = if f == MatFn::Rank {
                    steps.push(step(format!("\\operatorname{{rank}}={}", pivots.len()), "The number of pivots (leading 1s)"));
                    n(pivots.len() as i64)
                } else {
                    steps.push(step(matrix_latex(&reduced), "Reduced row echelon form"));
                    Sym::Matrix(reduced)
                };
                self.ev.work.push(Method { name: if f == MatFn::Rank { "Rank" } else { "Row reduction" }.into(), steps });
                result
            }
            MatFn::Dot => {
                need(2)?;
                let (u, v) = (matrix(&args[0])?, matrix(&args[1])?);
                self.dot(&u, &v)?
            }
            MatFn::Cross => {
                need(2)?;
                let (u, v) = (matrix(&args[0])?, matrix(&args[1])?);
                self.cross(&u, &v)?
            }
            MatFn::Norm => {
                need(1)?;
                let v = matrix(&args[0])?;
                self.norm(&v)?
            }
            MatFn::Eigenvalues | MatFn::Eigenvectors => {
                return Err(EngineError::new(format!("{} gives a list: put it on a line by itself", f.name())));
            }
        })
    }

    /// |v| = √(Σ vᵢ²)
    pub(crate) fn norm(&mut self, v: &M) -> R<Sym> {
        if !is_vector(v) {
            return Err(EngineError::new("The length |v| is for vectors; for a square matrix |A| is the determinant"));
        }
        let squares = simp(add(flat(v).into_iter().map(|e| pow(e, n(2))).collect::<R<Vec<_>>>()?));
        let value = pow(squares.clone(), sym::q(1, 2))?;
        self.ev.work.push(Method { name: "Length".into(), steps: vec![step(format!("\\sqrt{{{}}}={}", latex(&squares), latex(&value)), "The square root of the sum of the squares")] });
        Ok(value)
    }

    /// |A|: the determinant of a square matrix, or the length of a vector.
    pub(crate) fn matrix_abs(&mut self, m: &M) -> R<Sym> {
        if is_vector(m) && dims(m) != (1, 1) { self.norm(m) } else { self.det(m) }
    }

    /// Eigenvalues: the distinct roots of det(λI − A) = 0, with the working.
    pub(crate) fn eigenvalues(&mut self, m: &M) -> R<(Vec<crate::solve::Root>, Vec<Step>)> {
        let k = square(m, "Eigenvalues")?;
        let lambda = "\\lambda".to_string();
        let c = characteristic(m)?;
        let p = simp(add(c.iter().enumerate().map(|(i, ci)| mul(vec![ci.clone(), sym::pow_or_keep(Sym::Var(lambda.clone()), n(i as i64))])).collect()));
        let mut steps = vec![step(
            format!("\\det\\left(\\lambda I-A\\right)={}", latex(&p)),
            format!("The characteristic polynomial (degree {k})"),
        )];
        let sol = crate::solve::solve(self, vec![(p, n(0))], vec![])?;
        for meth in sol.methods.iter().take(1) {
            steps.extend(meth.steps.iter().skip(1).cloned());
        }
        let roots = match sol.outcome {
            crate::solve::Outcome::Sets(sets) => sets.into_iter().filter_map(|mut s| s.pop()).collect(),
            _ => Vec::new(),
        };
        Ok((roots, steps))
    }

    /// A basis of the null space of A − λI.
    pub(crate) fn eigenvectors_for(&mut self, m: &M, lambda: &Sym) -> R<Vec<M>> {
        let k = m.len();
        let shifted: M = (0..k).map(|i| (0..k).map(|j| simp(if i == j { add(vec![m[i][j].clone(), neg(lambda.clone())]) } else { m[i][j].clone() })).collect()).collect();
        let (reduced, pivots, _) = rref(shifted);
        let free: Vec<usize> = (0..k).filter(|c| !pivots.contains(c)).collect();
        let mut basis = Vec::new();
        for &fcol in &free {
            let mut v = vec![vec![n(0)]; k];
            v[fcol][0] = n(1);
            for (r, &pc) in pivots.iter().enumerate() {
                v[pc][0] = simp(neg(reduced[r][fcol].clone()));
            }
            basis.push(clear_denominators(v));
        }
        Ok(basis)
    }
}

/// A vector scaled so rational entries become whole numbers: (½, 1) → (1, 2).
fn clear_denominators(v: M) -> M {
    use num_integer::Integer;
    let lcm = v.iter().flatten().filter_map(|e| e.as_num()).fold(num_bigint::BigInt::from(1), |l, q| l.lcm(q.denom()));
    if lcm == 1.into() {
        return v;
    }
    scale(&v, &Sym::Num(Q::from_integer(lcm)))
}

fn wrap(s: &Sym) -> String {
    match s {
        Sym::Num(v) if v >= &Q::zero() => latex(s),
        Sym::Var(_) | Sym::Const(Const::Pi | Const::E) => latex(s),
        _ => format!("\\left({}\\right)", latex(s)),
    }
}
