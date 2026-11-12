<div align="center">

# ProCalc

**A calculator that shows its working.**

Exact answers, any precision you need, and the steps behind every result,
for students, teachers, engineers and mathematicians alike.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux-informational)
![Built with Rust](https://img.shields.io/badge/engine-Rust-orange)
![Tauri](https://img.shields.io/badge/app-Tauri%202%20%2B%20Svelte%205-24C8DB)
[![Latest release](https://img.shields.io/github/v/release/KimsFerdy/ProCalc)](https://github.com/KimsFerdy/ProCalc/releases/latest)

**[⬇ Download for Windows or Linux](https://github.com/KimsFerdy/ProCalc/releases/latest)**

</div>

---

## Why ProCalc?

Most calculators hand you a decimal and stop there. ProCalc treats maths the way you write it
on paper:

- **Exact first.** `0.1 + 0.2` is `3/10`, `√8` is `2√2`, and `sin 30°` is `½`. You get the
  decimal as well, but nothing is quietly rounded away.
- **Precise when you need it.** Up to 1000 significant digits. ProCalc recomputes at higher
  precision until every digit it shows is certain, so even `sin(10⁵⁰)` comes out right.
- **It shows the working.** Derivatives name each rule, integrals show the substitution or the
  parts, equations offer several methods (quadratic formula, completing the square,
  elimination, Cramer's rule, and more), and you choose which one to read.
- **Write maths as maths.** Type with a real maths editor (fractions, roots, integrals,
  matrices), or just type `sqrt(2)*pi`. Both work.
- **Honest.** If an integral has no closed form, ProCalc says so and integrates numerically.
  A series that can't be summed reliably is refused, not guessed. Every antiderivative is
  differentiated back and checked before you see it.

## What it can do

| Area | Highlights |
|---|---|
| **Arithmetic** | Exact fractions and surds, exact factorials up to 20000!, Γ for non-integers (`2.5! = 15√π/8`) |
| **Algebra** | Expand, factor, simplify, partial fractions, polynomial long division |
| **Equations** | Linear, quadratic and higher-degree equations, radical and trigonometric equations (every solution, `+2πk`), systems of up to 10 unknowns, nonlinear systems, inequalities by sign chart |
| **Calculus** | Derivatives of any order, indefinite and definite integrals, improper integrals, limits with L'Hôpital, Taylor series, sums and products in closed form (`Σ 1/k² = π²/6`) |
| **Differential equations** | First-order linear, separable, constant coefficients of any order, initial conditions, and a numerical fallback |
| **Linear algebra** | Determinants, inverses, row reduction with named row operations, rank, eigenvalues and eigenvectors, dot and cross products |
| **Complex numbers** | Exact `a + bi`, polar and angle form (`2∠60°`), every n-th root, principal branches throughout |
| **Number theory** | gcd/lcm with Euclid's algorithm, prime factorisation, primality, totient, nCr and nPr |
| **Statistics** | Mean, median, quartiles, variance, regression, binomial, Poisson and normal distributions |
| **Units** | `60 mph → km/h`, SI prefixes from quecto to quetta, temperatures, and units carried through calculations |
| **Optimisation** | Minima and maxima in one or several variables, on intervals or with constraints (Lagrange multipliers) |
| **Geometry** | Solve any triangle (including the ambiguous case), shapes, coordinate geometry, and a drawing workspace that turns a sketch and a few measurements into a figure drawn to scale |
| **Graphs** | Functions, implicit curves, parametric and polar curves, shaded inequalities and integrals, tangent lines, and 3D surfaces |

The full list, with the automated test behind each feature, is in **[FEATURES.md](FEATURES.md)**.

## A taste

Type these into a line on the Paper:

```text
\frac{d}{dx} x^x                          →  xˣ(ln x + 1), with each rule named
\int_0^1 e^{-x^2} dx                      →  0.74682413281242702539946…
x^2 - 5x + 6 = 0                          →  x = 2 or x = 3  (three methods to choose from)
\sum_{k=1}^{\infty} 1/k^2                 →  π²/6
\lim_{x\to0} (\tan x - x)/x^3             →  1/3
\det \begin{pmatrix}1&2\\3&4\end{pmatrix} →  −2
factor(360)                               →  2³ · 3² · 5
60\,\mathrm{mph} \to km/h                 →  96.56064 km/h
```

In the app you rarely type the LaTeX: the maths editor builds fractions, integrals and
matrices as you type, and writing `"mph"` marks a unit.

## The Paper

ProCalc's main workspace is the **Paper**: a sheet of live lines, like a notebook that
calculates.

- Every line is answered as you type.
- Define things once and use them below: `g := 9.81 m/s^2`, `f(x) := x^2 + 1`.
- Any line that is a function of one letter appears on the graph straight away.
- Mix maths lines with plain-text notes.
- Save your work as a project and reopen it later.

There is also a **Geometry** workspace: sketch a triangle, polygon or circle, type in the
measurements you know, and ProCalc finds the rest and redraws the figure to scale.

## Download

Get the latest version from the **[Releases page](https://github.com/KimsFerdy/ProCalc/releases/latest)**.

| System | Recommended | Also available |
|---|---|---|
| **Windows 10 / 11** | `…_x64-setup.exe` | `.msi` installer, portable `.zip` (no install) |
| **Linux** | `.AppImage` (any distribution) | `.deb` (Ubuntu, Mint, Debian), `.rpm` (Fedora, openSUSE), portable `.zip` / `.tar.gz` |

The Windows build isn't code-signed yet, so SmartScreen may warn you: choose
**More info → Run anyway**. macOS should work when built from source but hasn't been tested.

## Building from source

**You'll need:**

- [Node.js](https://nodejs.org/) 20 or later
- [Rust](https://rustup.rs/) (stable)
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your system. On
  Windows that means the Visual Studio C++ build tools and WebView2.

**Then:**

```bash
git clone https://github.com/KimsFerdy/ProCalc.git
cd ProCalc
npm install
npm run tauri dev      # run the app in development mode
npm run tauri build    # build an installer (in target/release/bundle)
```

## For developers

ProCalc has two parts:

```text
engine/      The maths engine: a standalone Rust crate (parsing, exact and
             arbitrary-precision arithmetic, symbolic algebra, calculus, solving)
src-tauri/   The desktop shell (Tauri 2)
src/         The interface (SvelteKit + Svelte 5, MathLive for maths input)
```

The engine has no UI dependencies and can be used on its own:

```bash
cargo test -p procalc-engine                       # the full test suite
cargo run -p procalc-engine --example stress       # ~480 hostile and edge cases, with hang detection
bash scripts/check-features.sh                     # FEATURES.md and the tests must match
```

Every feature in [FEATURES.md](FEATURES.md) names the test that proves it, and
`check-features.sh` makes sure that stays true.

## Design choices

A few deliberate conventions, so they aren't mistaken for bugs:

- `e` is always Euler's number and `i` is always the imaginary unit. Use `i_1` for a variable.
- Odd roots of negative numbers are real (`∛−8 = −2`), as on most calculators.
- Variables are one letter (with an optional subscript like `v_{max}`). Functions can have
  longer names.

[FEATURES.md](FEATURES.md#known-limitations) has the complete list.

## Contributing

Bug reports, odd results and ideas are all welcome. Please
[open an issue](https://github.com/KimsFerdy/ProCalc/issues) and include the exact input that
surprised you. For a wrong answer, the expected result and where it comes from help a lot.

## License

ProCalc is released under the [MIT License](LICENSE).
