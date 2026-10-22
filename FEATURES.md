# ProCalc feature map

Every finished feature names the test that proves it. When a test fails, search this file
for its name to see which feature broke.

```bash
cargo test -p procalc-engine              # run everything
cargo test -p procalc-engine surds        # run tests whose name contains "surds"
scripts/check-features.sh                 # check this file and the test suite still match
```

**Legend:** `[x]` done and tested · `[~]` partly done · `[ ]` planned · *(manual)* checked by hand, no automated test yet

Test paths are relative to `engine/src/`: `tests::` is in `lib.rs`, `tests_symbolic::` in
`tests_symbolic.rs`, `parser::tests::` in `parser.rs`, `number::tests::` in `number.rs`.

---

## 1. Input and parsing

- [x] MathLive LaTeX: `\frac`, `\sqrt`, `\sqrt[n]`, `\left( \right)`, `\cdot`, `\times`, `\div`, `\operatorname`, `\exponentialE`, digit spacing `1\,000` — `parser::tests::mathlive_output`
- [x] Plain typed text: `sqrt(2)*pi`, `sin(30)+cos(60)` — `parser::tests::plain_words`
- [x] LaTeX rule that an unbraced exponent takes one character (`2^10` is 2¹·0) — `parser::tests::unbraced_exponent_takes_one_digit`
- [x] `\frac12` shorthand — `parser::tests::frac_shorthand`
- [x] An empty MathLive box gives a clear error — `parser::tests::placeholder_is_an_error`
- [x] Variables: single letters, Greek (`\theta`), subscripts (`x_1`, `v_{max}`); `xy` means x·y — `parser::tests::variables`
- [x] Operator precedence, implicit multiplication `2(3+4)`, `%`, `|x|` — `tests::precedence`
- [x] Degree sign `30°` / `30^{\circ}` works in either angle mode — `tests::trig_degrees`
- [ ] Assignments: `x := 5`, `f(x) := x^2`
- [ ] Units: `5\,\mathrm{kg}`
- [ ] Complex numbers (`i` is currently an ordinary variable)

## 2. Exact arithmetic

- [x] Fractions stay exact: 0.1 + 0.2 = 3/10 — `tests::floating_point_traps_are_exact`
- [x] Exact powers and roots: 8^(2/3) = 4, ∛(−27) = −3, 2¹⁰⁰ written in full — `tests::exact_roots_and_powers`
- [x] Factorial, exact, up to 20000! — `tests::factorial_is_exact`
- [x] Double factorial n!! — `tests::double_factorial`
- [ ] Gamma function (factorial of non-integers)
- [ ] nCr, nPr, gcd, lcm, mod, prime factorisation

## 3. Precision and accuracy

- [x] Correct rounding of repeating decimals — `tests::rounding_of_repeating_decimals`, `number::tests::rational_rounding`
- [x] π, e, √2, ln 2 correct to 32 digits — `tests::constants_to_32_digits`, `tests::logs`
- [x] Up to 1000 significant digits (π to 100 checked) — `tests::high_precision`
- [x] **Precision guarantee:** decimals are computed at rising precision until two runs agree on every shown digit, so huge angles like sin(10⁵⁰) are still right (checked against `bc`) — `tests::huge_angles_stay_accurate`
- [x] The symbolic and purely numeric paths agree digit for digit (16/32/100 digits, RAD/DEG) — `tests_symbolic::symbolic_and_numeric_paths_agree`
- [x] Scientific notation beyond 10²¹ and below 10⁻⁶ — `tests::scientific_notation`
- [x] Thin-space digit grouping — `number::tests::grouping`

## 4. Functions

- [x] sin/cos/tan in radians; sin(π) shows exactly 0 — `tests::trig_radians`
- [x] Degree mode, with exact values like sin 30° = ½ and tan 90° reported as undefined — `tests::trig_degrees`
- [x] Inverse trig, including `\sin^{-1}` notation — `tests::trig_degrees`, `tests_symbolic::exact_inverse_trig`
- [x] ln, log₁₀, log_b with exact results where possible (log₂ 8 = 3, log₈ 2 = ⅓) — `tests::logs`
- [~] Hyperbolic functions and their inverses: they evaluate, but only special values are tested
- [x] abs, floor, ceil — `tests::precedence`, `tests_symbolic::absolute_value`
- [ ] Gamma, erf, Bessel and other special functions

## 5. Symbolic core

- [x] Collect like terms: x + x = 2x, x − x = 0 — `tests_symbolic::like_terms_collect`
- [x] Combine powers: x·x = x², x⁵/x² = x³, (2x)² = 4x² — `tests_symbolic::powers_combine`
- [x] Terms printed in polynomial order (x² + x + 1) — `tests_symbolic::polynomial_order`
- [x] Quotients printed as proper fractions — `tests_symbolic::fractions_print`
- [x] Surds simplified: √8 = 2√2, √2·√3 = √6, ⁴√4 = √2 — `tests_symbolic::surds_simplify`
- [x] Rationalised denominators: 1/√2 = √2/2 — `tests_symbolic::denominators_rationalise`
- [x] Roots of variables: √(4x) = 2√x, √x·√x = x — `tests_symbolic::roots_of_variables`
- [x] Exact trig at multiples of π/6 and π/4 — `tests_symbolic::exact_trig_radians`, `tests_symbolic::exact_trig_degrees`
- [x] Exact inverse trig: arcsin ½ = π/6, arctan √3 = π/3 — `tests_symbolic::exact_inverse_trig`
- [x] Log and exp rules: ln eˣ = x, e^(ln x) = x, log_b bˣ = x — `tests_symbolic::logs_and_exp`
- [x] |−3x| = 3|x|, |−π| = π — `tests_symbolic::absolute_value`
- [x] π and e stay symbolic: π + π = 2π, e·e = e² — `tests_symbolic::constants_stay_symbolic`
- [x] Greek letters and subscripts in results — `tests_symbolic::greek_and_subscripts`
- [x] No pointless "exact" form that just repeats the input (sin 1) — `tests_symbolic::unevaluated_functions_hide_exact`
- [x] Results with variables have no decimal line; x − x + 1 does — `tests_symbolic::symbolic_results_have_no_decimal`
- [x] Errors with variables (x/0, log₁ x) — `tests_symbolic::symbolic_errors`
- [ ] Expand: (x+1)² → x² + 2x + 1
- [ ] Factor: x² − 1 → (x−1)(x+1)
- [ ] Solve equations: x² = 2 → x = ±√2
- [ ] Derivatives
- [ ] Integrals
- [ ] Substitution: evaluate `2x+1` at x = 3
- [ ] Plotting

## 6. Robustness

- [x] Bad input gives a clear error, never a crash: 1/0, √−1, ln 0, `2+`, empty input — `tests::errors`
- [x] Hostile input returns quickly without panicking: power towers, (√2)¹⁰⁰⁰⁰⁰⁰, 10!!, 0⁰, sin(10⁵⁰) — `tests_symbolic::nasty_inputs_never_panic`

## 7. App (desktop UI)

- [x] *(manual)* MathLive input with live preview while typing
- [x] *(manual)* Enter saves to history; history persists between launches
- [x] *(manual)* DEG/RAD toggle and a digits picker (16 to 1000)
- [x] *(manual)* Click a result to reuse it; Copy button
- [x] *(manual)* Symbolic results show the simplified expression only
- [x] *(manual)* History saved by v0.1 still loads (`migrateAnswer`)
- [ ] Automated UI tests
- [ ] Matrices: editable grid input and rendering
- [ ] Physics, chemistry and robotics modules

## 8. Platforms

- [x] *(manual)* Linux desktop (Linux Mint 22.3)
- [ ] Windows
- [ ] macOS
- [ ] Android
- [ ] iOS

---

## Known limitations

Deliberate simplifications, so they don't get mistaken for bugs:

- **x/x = 1 and x⁰ = 1** assume x ≠ 0. Most computer algebra systems make the same choice.
- **x^a · x^b = x^(a+b)** even for fractional powers, which assumes x ≥ 0 where it matters.
- **(x²)^½ stays as written** rather than becoming |x|.
- **sin(−x) is not rewritten** as −sin(x) yet.
- **(x+1)¹⁰⁰ is not expanded**; expansion will be a separate, explicit operation.
- **Unknown multi-letter words become products:** `abc` = a·b·c, while known names (sin, sqrt, pi) are recognised.
- **`e` is always Euler's number** and can't be used as a variable name.
- **`i` is an ordinary variable** until complex numbers arrive.
- **Integer powers are only expanded up to about 10⁶ bits** (300k digits). Larger ones stay symbolic but still get a decimal.
