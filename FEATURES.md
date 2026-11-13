# ProCalc feature map

Every finished feature names the test that proves it. When a test fails, search this file
for its name to see which feature broke.

```bash
cargo test -p procalc-engine              # run everything
cargo test -p procalc-engine surds        # run tests whose name contains "surds"
scripts/check-features.sh                 # check this file and the test suite still match
```

**Legend:** `[x]` done and tested · `[~]` partly done · *(manual)* checked by hand, no automated test yet

Test paths are relative to `engine/src/`: `tests::` is in `lib.rs`, `tests_symbolic::` in
`tests_symbolic.rs`, `tests_defs::` in `tests_defs.rs`, `tests_complex::` in
`tests_complex.rs`, `tests_units::` in `tests_units.rs`, `tests_calculus::` in `tests_calculus.rs`,
`tests_solve::` in `tests_solve.rs`, `tests_steps::` in `tests_steps.rs`, `tests_plot::` in `tests_plot.rs`,
`tests_inequalities::` in `tests_inequalities.rs`, `tests_numtheory::` in `tests_numtheory.rs`,
`tests_series::` in `tests_series.rs`, `tests_matrix::` in `tests_matrix.rs`, `tests_ode::` in `tests_ode.rs`, `tests_stats::` in `tests_stats.rs`, `tests_geometry::` in `tests_geometry.rs`, `tests_figure::` in `tests_figure.rs`,
`parser::tests::` in `parser.rs`,
`number::tests::` in `number.rs`, `poly::tests::` in `poly.rs`, `ratfunc::tests::` in `ratfunc.rs`.

---

## 1. Input and parsing

- [x] MathLive LaTeX: `\frac`, `\sqrt`, `\sqrt[n]`, `\left( \right)`, `\cdot`, `\times`, `\div`, `\operatorname`, `\exponentialE`, digit spacing `1\,000` — `parser::tests::mathlive_output`
- [x] Plain typed text: `sqrt(2)*pi`, `sin(30)+cos(60)` — `parser::tests::plain_words`
- [x] LaTeX rule that an unbraced exponent takes one character (`2^10` is 2¹·0) — `parser::tests::unbraced_exponent_takes_one_digit`
- [x] `\frac12` shorthand — `parser::tests::frac_shorthand`
- [x] An empty MathLive box gives a clear error — `parser::tests::placeholder_is_an_error`
- [x] Variables: single letters, Greek (`\theta`), subscripts (`x_1`, `v_{max}`); `xy` means x·y — `parser::tests::variables`
- [x] Operator precedence, implicit multiplication `2(3+4)`, `%`, `|x|` — `tests::precedence`
- [x] `\sin(x)^2` is (sin x)², `\sin x^2` is sin(x²) — `parser::tests::function_power_binds_outside_brackets`
- [x] Degree sign `30°` / `30^{\circ}` works in either angle mode — `tests::trig_degrees`
- [x] Variables: `x := 5` (MathLive's `\coloneq` too) carries to later lines, values fixed when defined — `tests_defs::variables_carry_to_later_lines`, `tests_defs::definitions_report_what_they_define`
- [x] Functions: `f(x) := x^2`, `area(w,h) := wh`, `\phi(t) := 2t`, built from each other — `tests_defs::functions_can_be_called`
- [x] Parameters shadow variables; a function sees only earlier definitions — `tests_defs::function_scope_rules`
- [x] Bad definitions are clear errors (`sin := 3`, `xy := 3`, `x = 5`); earlier lines that broke are skipped — `tests_defs::bad_definitions_are_errors`, `tests_defs::broken_earlier_lines_are_skipped`
- [x] Deeply nested functions stop with an error instead of hanging — `tests_defs::deeply_nested_functions_stop_quickly`
- [x] `i` (and MathLive's `\imaginaryI`, `\imaginaryJ`) is the imaginary unit; `i_1`, `e_1` are variables — `tests_complex::i_squared_is_minus_one`, `tests_complex::i_with_a_subscript_is_a_variable`

## 2b. Complex numbers

- [x] Exact a + bi arithmetic, including division: 1/(1+i) = ½ − ½i — `tests_complex::exact_complex_arithmetic`
- [x] Roots of negative numbers: √−4 = 2i, ⁴√−16 = √2 + √2i (odd roots stay real) — `tests_complex::roots_of_negative_numbers`
- [x] Euler and logs: e^{iπ} = −1, ln(−1) = πi, i^i = e^{−π/2} — `tests_complex::euler_and_logs`
- [x] Modulus |3+4i| = 5 — `tests_complex::modulus`
- [x] Every function takes complex input; asin 2, acosh ½, log(−100) give principal values (Mathematica's branch cuts) — `tests_complex::functions_beyond_the_real_domain`
- [x] The symbolic and numeric paths agree on complex results — `tests_complex::symbolic_and_numeric_paths_agree_on_complex`
- [x] Bad and hostile complex input fails cleanly and quickly — `tests_complex::complex_errors_and_hostile_input`
- [x] Re, Im and the conjugate (`conj`, `\overline{z}`), exact: (3+4i)·conj(3+4i) = 25 — `tests_complex::real_and_imaginary_parts_and_conjugate`
- [x] arg, exact at the standard angles, in degrees in DEG mode: arg(−1−√3i) = −2π/3 — `tests_complex::argument_of_a_complex_number`
- [x] `polar(z)` = re^{iθ} (r∠θ° in DEG mode) with steps: polar(−3+4i) = 5e^{i(π−arctan 4/3)} — `tests_complex::polar_form`
- [x] Angle notation as input: 2∠60° = 1 + √3i — `tests_complex::angle_notation`
- [x] `roots(z, n)`: all n-th roots by De Moivre, exact at standard angles (roots(8,3) = 2, −1 ± √3i) — `tests_complex::all_nth_roots`
- [x] xⁿ = c with no real root gives the complex roots, like x² = −4 does; complex roots fail conditions like x > 0 — `tests_complex::equations_with_only_complex_roots`

## 2e. Statistics and probability

- [x] mean, median, mode (all modes on a tie), sample and population variance and σ, min, max, range, sum, count — exact, with steps; a list `[…]`, a defined list, or the numbers themselves — `tests_stats::centre_and_spread`
- [x] Quartiles (TI method), IQR, percentiles (linear interpolation) — `tests_stats::quartiles_and_percentiles`
- [x] Correlation, covariance, `linreg(xs, ys)` with a, b, r, r² — `tests_stats::correlation_and_regression`
- [x] `stats(L)` summary; lists defined on the Paper — `tests_stats::summary_and_named_lists`
- [x] binompdf/cdf, poissonpdf/cdf, geompdf/cdf, exact — `tests_stats::discrete_distributions`
- [x] normalpdf, normalcdf (through erf), invnorm (Newton on Φ), erf at any precision with its derivative — `tests_stats::normal_distribution_and_erf`

## 2d. Matrices and vectors

- [x] Matrices from pmatrix/bmatrix (trailing `\\` allowed); sums, scalar multiples, products (order kept), powers, Aᵀ; a decimal matrix answer too — `tests_matrix::matrix_input_and_arithmetic`
- [x] Determinants with steps: ad − bc, cofactor expansion, elimination for larger ones; vmatrix and |A|; letters allowed, and det(…) = 0 solves — `tests_matrix::determinants`
- [x] Inverses: the 2×2 formula, Gauss–Jordan on [A | I] in steps, symbolic entries, singular matrices reported — `tests_matrix::inverses`
- [x] rref with named row operations, rank, trace, identity(n) — `tests_matrix::row_reduction_rank_and_trace`
- [x] Eigenvalues from the characteristic polynomial (exact, complex too) and eigenvectors by row reduction — `tests_matrix::eigenvalues_and_eigenvectors`
- [x] Vectors `[1,2,3]` and `\langle 1,2\rangle`: dot (·), cross (×), length |v|, norm — `tests_matrix::vectors`
- [x] Definitions on the Paper (`A := …`, then `AB`, `A^{-1}`, `\det(A)`), entry-wise derivatives, and the graph leaves matrices alone — `tests_matrix::matrices_on_the_paper`

## 2c. Units

- [x] Units in `\mathrm{}` / `\text{}` (`5\,\mathrm{kg}`, `\mathrm{km/h}`, `\text{N m}`) combine like symbols: 5 kg + 3 kg, √(9 m²) = 3 m, cancelling units leave a number — `tests_units::unit_arithmetic`
- [x] Sums of different units of one kind use the largest: 1 km + 300 m = 1.3 km — `tests_units::mixed_units_use_the_largest`
- [x] Conversion with `\to` / `→` (MathLive turns `->` into it); after the arrow, plain letters are units: 60 mph → km/h — `tests_units::conversions`
- [x] SI prefixes from quecto to quetta, µ (or u), Ω, and unit groups split up: kWh, µm, kΩ, `\text{N m}` — `tests_units::prefixes_and_spellings`
- [x] Temperatures: 20 °C → °F converts with offsets; in J/(g·°C) a degree is a difference; mixing °C/°F/K in a sum is an error — `tests_units::temperatures`
- [x] Definitions and variables carry units: `a := 9.81\,\mathrm{m/s^2}`, (x+2) m — `tests_units::units_with_definitions_and_variables`
- [x] Clear errors: adding kg to m, converting m to s, units in sin or an exponent — `tests_units::unit_errors`
- [~] *(manual)* The app types common multi-letter units (kg, km, kWh…) as units, and `"` starts and ends a unit for the rest (`"m"`) — built, not yet checked by hand

## 2. Exact arithmetic

- [x] Fractions stay exact: 0.1 + 0.2 = 3/10 — `tests::floating_point_traps_are_exact`
- [x] Exact powers and roots: 8^(2/3) = 4, ∛(−27) = −3, 2¹⁰⁰ written in full — `tests::exact_roots_and_powers`
- [x] Factorial, exact, up to 20000! — `tests::factorial_is_exact`
- [x] Double factorial n!! — `tests::double_factorial`
- [x] Factorial of non-integers through Γ: 2.5! = 15√π/8, π! ≈ 7.188 — `tests_numtheory::factorial_of_non_integers`

## 2a. Number theory

- [x] gcd (Euclid's algorithm in the steps) and lcm, any number of arguments; `\gcd`, `\operatorname{lcm}`, `hcf` — `tests_numtheory::gcd_and_lcm`, `tests_numtheory::gcd_shows_euclids_algorithm`
- [x] mod, infix (`17 \bmod 5`, `17 mod 5`) or as `mod(a, b)`; never negative (−7 mod 3 = 2) and works on fractions — `tests_numtheory::mod_is_never_negative`
- [x] nCr (`\binom{n}{k}`, `nCr(n, k)`) and nPr, exact — `tests_numtheory::binomials_and_permutations`
- [x] Euler's totient φ(n) — `tests_numtheory::totient`
- [x] Prime factorisation `factor(360)` = 2³·3²·5 with steps; Pollard's rho for large factors — `tests_numtheory::factor_integers_into_primes`
- [x] `isprime(n)` (Miller–Rabin; "not prime: 91 = 7·13") and `divisors(n)` — `tests_numtheory::primes_and_divisors`
- [x] Γ(x): exact at whole numbers and half-integers (Γ(½) = √π), any precision elsewhere (Stirling's series); `\Gamma` without a bracket is still the letter — `tests_numtheory::gamma_function`

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
- [~] Special functions: Γ (see 2a) and erf (see 2e) are done; Bessel and the rest are not

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
- [x] Expand: (x+1)² → x² + 2x + 1, with a size limit — `tests_symbolic::expand_products_and_powers`, `tests_symbolic::expand_has_a_size_limit`
- [x] Factor: x² − 1 → (x−1)(x+1), several variables, functions as variables — `tests_symbolic::factor_polynomials`, `tests_symbolic::factor_several_variables`, `tests_symbolic::factor_treats_functions_as_variables`
- [x] Factor leaves irreducibles and plain numbers alone, and its output re-parses — `tests_symbolic::factor_leaves_irreducibles_alone`, `tests_symbolic::factor_leaves_symbols_alone`, `tests_symbolic::factor_round_trips_through_latex`
- [x] Fractions with letters cancel automatically, with steps: (x²−1)/(x−1) = x + 1, (2x+4)/6 = (x+2)/3, (x²−y²)/(x−y) = x + y — `tests_symbolic::fractions_with_letters_cancel`
- [x] `simplify` puts everything over one denominator and cancels — `tests_symbolic::simplify_combines_fractions`
- [x] `apart`: partial fractions with repeated and quadratic factors, dividing first when needed, in steps — `tests_symbolic::partial_fractions`
- [x] `divide(p, d)`, `quotient`, `remainder`: long division of polynomials (or whole numbers), step by step — `tests_symbolic::polynomial_long_division`
- [x] Polynomial building blocks: division, GCD, square-free split (Yun), exact division in several letters — `poly::tests::poly_division`, `poly::tests::poly_gcd`, `poly::tests::yun_square_free`, `ratfunc::tests::exact_division_in_several_letters`
## 5a. Equations

- [x] Type `=` for an equation (`:=` still defines); linear and quadratic, complex roots, letters as parameters — `tests_solve::linear_and_quadratic_equations`
- [x] Quadratics offer three methods in Steps: quadratic formula, completing the square, factoring — `tests_solve::quadratic_methods_to_choose_from`
- [x] Higher degrees by factoring (exact ∛2 for x³ = 2), rational equations with excluded roots — `tests_solve::higher_degree_and_rational_equations`
- [x] Isolating the unknown: e^x, 2^x, ln, √, |…|, and trig with every solution (+2πk, k ∈ ℤ) — `tests_solve::isolating_the_unknown`
- [x] Radical equations square both sides and drop the roots that squaring added — `tests_solve::radical_equations_drop_extraneous_roots`
- [x] Numeric roots when nothing exact applies (cos x = x); the 10 nearest 0 when there are many — `tests_solve::numeric_roots_when_nothing_exact_applies`
- [x] Identities and contradictions: true for every x, no solution, true/false — `tests_solve::identities_and_contradictions`
- [x] Linear systems of 2–10 equations (`,` `;` or a cases block), no-solution and free-variable cases — `tests_solve::linear_systems`, `tests_solve::ten_unknowns`
- [x] Five methods to choose from for systems: substitution, elimination, Gauss–Jordan, Cramer's rule, inverse matrix — `tests_solve::system_methods_to_choose_from`
- [x] Nonlinear systems by substitution: x + y = 5, xy = 6 — `tests_solve::nonlinear_systems`
- [x] Clear errors for malformed equations — `tests_solve::equation_errors`
- [~] *(manual)* Steps view on the Paper with a method picker per line (equations, derivatives, integrals) — built, not yet checked by hand
- [x] One-letter inequalities by sign chart, with exact endpoints and open/closed ends — `tests_inequalities::one_letter_inequalities_by_sign_chart`
- [x] Denominators and domain edges (√, ln) split the number line — `tests_inequalities::denominators_and_domains_split_the_line`
- [x] Chains (1 < 2x+1 ≤ 7), true/false, two-letter regions (shown on the graph) — `tests_inequalities::chains_regions_and_truth`
- [x] Conditions after an equation keep the solutions in range, expanding +2πk into the actual values — `tests_inequalities::conditions_pick_out_solutions`

## 5b. Calculus

- [x] Derivatives `\frac{d}{dx}`, `\frac{\mathrm{d}}{\mathrm{d}x}`, any letter; power, product and chain rules, every built-in function, x^x — `tests_calculus::derivative_rules`
- [x] Higher derivatives `\frac{d^2}{dx^2}` and primes on your own functions: `f'(2)`, `f''(x)`, `f^{\prime}` — `tests_calculus::higher_derivatives_and_primes`
- [x] With x defined, d/dx is the slope there; degree mode adds the π/180 — `tests_calculus::derivatives_at_defined_points`
- [x] Indefinite integrals with `+C`: table integrals, f(ax+b), 1/(x²+a²), 1/√(a²−x²) — `tests_calculus::indefinite_integrals`
- [x] u-substitution, integration by parts, expanding products, partial fractions; no antiderivative is an honest error — `tests_calculus::integration_techniques`
- [x] Other letters are constants; double integrals — `tests_calculus::other_letters_and_double_integrals`
- [x] Every antiderivative is differentiated back and checked numerically before it's shown
- [x] Definite integrals exactly via F(b) − F(a); an integrand undefined inside the interval is an error, not a wrong number — `tests_calculus::definite_integrals`
- [x] No closed form: tanh-sinh numeric integration to every digit shown (∫₀¹ e^{−x²}) — `tests_calculus::numeric_integrals`
- [x] Substitution `expr|_{x=3}`, `\left. … \right|_{x=1,y=2}`, and `F|_a^b` = F(b) − F(a) — `tests_calculus::substitution_bars`
- [x] Typed `∫`, `x²`, plain `d/dx`; MathLive's `int` shortcut with empty bound boxes, and its `\differentialD` — `tests_calculus::typed_symbols_and_mathlive_templates`
- [x] Clear errors for missing pieces (d/dx with nothing after it, ∫ without dx, one bound) — `tests_calculus::calculus_errors`
- [x] Integrals to ±∞, by lim F or numerically (x = u/(1−u) onto [0, 1)); singular ends like ∫₀¹ ln x; divergence reported — `tests_series::improper_integrals`

## 5g. Optimisation

- [x] `minimize`, `maximize`, `extrema` of one variable: f′ = 0, the second-derivative test or the sign of f′, unbounded functions reported, the value inside larger expressions — `tests_geometry::optimise_one_variable`
- [x] On an interval (`minimize(f, -3, 3)`, `0 \le x \le 2\pi`): the ends are candidates; periodic functions keep the critical points inside — `tests_geometry::optimise_on_an_interval`
- [x] Several variables: ∇f = 0 and the Hessian (minimum, maximum, saddle) — `tests_geometry::optimise_several_variables`
- [x] Constraints by Lagrange multipliers, λ eliminated for one constraint — `tests_geometry::optimise_with_constraints`

## 5h. Geometry

- [x] `triangle(a=…, B=…, …)`: SSS, SAS, ASA, AAS and both triangles in the ambiguous SSA case, with area and perimeter; angles in the angle mode — `tests_geometry::triangles`
- [x] Shapes: circle (r or d), square, rectangle, trapezoid, sphere, cylinder, cone, cube, cuboid; named or positional measurements — `tests_geometry::shapes`
- [x] Coordinates: distance, midpoint, slope, line, angle (two vectors or three points), polygon area; your own functions named like a shape still win — `tests_geometry::coordinates`

**Geometry workspace** (Math → Geometry): draw a figure, give what you know, get the rest.

- [x] Triangles solved exactly from any three measurements (with a side), the solution nearest the sketch chosen, extra values checked; drawn to scale with A and AB's direction kept; area, perimeter, height, circumradius, inradius with formula and working — `tests_figure::triangles_solve_exactly_and_to_scale`
- [x] What's missing ("Give 2 more sides or angles", "give a side"), what doesn't fit, and values that aren't numbers — `tests_figure::what_is_missing_or_wrong`
- [x] Rectangles and squares (90° fixed, opposite sides checked; area, perimeter, diagonal) and circles from r, d, C or A — `tests_figure::rectangles_squares_and_circles`
- [x] Any polygon from 2n − 3 measurements, solved numerically from the sketch (angle sum checked, drawn shape kept where two fit); perimeter and shoelace area — `tests_figure::any_polygon_adjusts_to_its_measurements`
- [~] *(manual)* The workspace: tools (triangle, rectangle, square, polygon, circle), grid snapping (Alt to place freely), corners labelled A, B, C…, click a side or angle to type its value (blank = unknown), drag corners, the figure redrawn to scale when solved; Inspector with given/found/unknown chips and "What do you want?" cards (value, decimal, formula, steps); areas also in m², ha, acres and ft²; saved in the project and between launches — built, not yet checked by hand

**Unit circle workspace** (Math → Unit circle): drag the angle and see sin, cos and tan.

- [x] *(manual)* Drag P round the circle (snapping to the standard angles, which can be turned off), arrow keys (Shift for 15°), or type an angle (`150`, `5π/6`, `2.1`, read in the angle mode unless it says ° or π); cos, sin and tan drawn as lengths, tan on the tangent line x = 1
- [x] *(manual)* The angle in degrees and radians side by side (an exact multiple of π at multiples of 15°), quadrant and reference angle; sin, cos and tan exact at the standard angles from the engine, with the decimal, and tan reported undefined at 90° and 270°; P's coordinates
- [x] *(manual)* sin and cos over one turn with θ marked on both curves; dragging along the graph moves the angle; the angle is saved in the project and between launches

## 5f. Differential equations

- [x] First-order linear by the integrating factor (`y'+y=x`, `xy'+y=x^2`); `y'`, `\frac{dy}{dx}`, `\frac{d}{dx}y`; t when the equation uses t; other letters are parameters — `tests_ode::first_order_linear_by_integrating_factor`
- [x] Separable equations, solved for y when possible (both branches), else implicit — `tests_ode::separable_equations`
- [x] Constant coefficients of any order: real, repeated and complex characteristic roots, letters (ω), quartics in r² — `tests_ode::constant_coefficients`
- [x] Particular solutions by undetermined coefficients (resonance included), or variation of parameters — `tests_ode::undetermined_coefficients`
- [x] Initial conditions `y(0)=1, y'(0)=0` fix the constants and pick the branch; y₀ stays a parameter — `tests_ode::initial_conditions`
- [x] Anything else with full initial conditions is solved numerically (RK4) — `tests_ode::numeric_solutions`
- [x] On the graph: particular solutions, a family for one free constant, numeric curves — `tests_ode::solutions_on_the_graph`

## 5e. Limits, series and Taylor

- [x] Limits by substitution and L'Hôpital's rule, with steps: lim sin x/x = 1, lim (tan x − x)/x³ = ⅓ — `tests_series::limits_by_substitution_and_lhopital`
- [x] Limits at ±∞: leading powers, (1 + 1/x)ˣ = e, √(x²+x) − x = ½, bounded/unbounded → 0 — `tests_series::limits_at_infinity`
- [x] 0·∞, ∞ − ∞, 1^∞, 0⁰ rewritten as quotients; one-sided limits (`x\to0^{+}`); ±∞ answers; "doesn't exist" when the sides differ — `tests_series::indeterminate_forms_and_sides`
- [x] Σ in closed form: Faulhaber (Σk = n(n+1)/2), geometric, a billion terms instantly — `tests_series::finite_sums_in_closed_form`
- [x] Infinite series: geometric, ζ(2m) exactly (π²/6, π⁴/90); others numerically (alternating: Cohen–Villegas–Zagier; fast: directly; power-law: cross-checked Richardson extrapolation) — `tests_series::infinite_series`
- [x] Divergence detected (terms not → 0, or like 1/k); series that can't be summed reliably are refused, not guessed — `tests_series::divergent_and_unsummable_series`
- [x] Π: term by term, cⁿ, Π k = n! — `tests_series::products`
- [x] `taylor(f, a, n)` with the derivative table; 0/0 at the point (sin x / x) by dividing series — `tests_series::taylor_series`

## 5c. Step-by-step working

- [x] Derivatives name each rule as it's used (sum, constant multiple, product, quotient, power, chain, exponential, each function) — `tests_steps::derivative_steps_name_each_rule`
- [x] Quotient rule working, with the answer's numerator multiplied out — `tests_steps::quotient_rule_steps_and_tidy_answer`
- [x] Higher derivatives round by round; derivatives at a defined point and `f'(2)` end with the value — `tests_steps::higher_derivatives_and_points_show_each_round`
- [x] Integrals show the technique that worked: term by term, power rule, standard integrals, u-substitution (u, du, rewrite, put back), integration by parts (u, dv, du, v), multiplying out — `tests_steps::integral_steps_show_the_technique`
- [x] Definite integrals end with $[F]_a^b = F(b) − F(a)$; numeric ones explain the quadrature — `tests_steps::definite_and_numeric_integral_steps`
- [x] Substitution bars show each replacement; a line with several calculus pieces gets one method each — `tests_steps::substitution_and_mixed_lines`


## 5d. Graphs

- [x] Every Paper line that is a function of one letter is sampled for the graph: expressions, `f(x) := …`, `y := …`, `y = …`; definitions feed later curves — `tests_plot::plottable_lines_are_sampled`, `tests_plot::definitions_feed_later_curves`
- [x] Undefined and complex points are gaps (1/x at 0, √x and ln x below 0) — `tests_plot::undefined_points_are_gaps`
- [x] Sampling is quick enough to pan and zoom (small whole powers by multiplication, eˣ through exp) — `tests_plot::sampling_is_fast_enough_to_pan`
- [~] *(manual)* Graph panel on the Paper: validated colour per line (kept while the line exists), legend chips to hide curves, line-number labels on the curves, drag to pan, scroll/pinch to zoom (Shift: x only, Alt: y only), double-click or Fit for the height, hover crosshair with values, roots and crossings marked — built, not yet checked by hand
- [x] A definite integral line plots its integrand with the area shaded; `f'(a)` and `\left.\frac{d}{dx}f\right|_{x=a}` draw the tangent at a — `tests_plot::integrals_shade_and_derivatives_draw_tangents`
- [x] Implicit curves from any equation in two letters (x² + y² = 1, xy = 1), parametric `x = f(t), y = g(t)` and polar `r = f(θ)` (including r = 2) — `tests_plot::implicit_parametric_and_polar_curves`
- [~] *(manual)* Drawing them: marching squares for implicit curves, shaded areas, dashed tangents with their touching point and slope on hover — built, not yet checked by hand
- [x] Inequalities shade their region (`y > x^2`, `x^2+y^2 \le 4, y > 0`); conditions set ranges (`x=\cos t, y=\sin t, 0 \le t \le \pi`, `r=\theta, 0\le\theta\le 4\pi`) and limit curves (`y = x^2, -1 \le x \le 1`); a function of two letters or `z = f(x, y)` is a 3D surface — `tests_plot::inequalities_ranges_and_surfaces`
- [~] *(manual)* Shaded regions (dashed edge when strict), the 3D view (drag to turn, scroll to zoom) — built, not yet checked by hand
- [~] *(manual)* 2D/3D switch whenever a surface is on the sheet; in 2D a surface is a heat map with z on hover; the 3D toolbar toggles mesh lines, style (shaded / heat / wire), the box, spin and perspective, remembered between launches — built, not yet checked by hand

## 6. Robustness

- [x] Bad input gives a clear error, never a crash: 1/0, 0⁻¹, ln 0, `2+`, empty input — `tests::errors`
- [x] *(stress)* A corpus of ~480 cases across every feature runs clean, each in its own thread with panic and hang detection: `cargo run -p procalc-engine --example stress` (see `stress/corpus.txt`; the listing goes to `stress/report.md`)
- [x] Hostile input returns quickly without panicking: power towers, (√2)¹⁰⁰⁰⁰⁰⁰, 10!!, 0⁰, sin(10⁵⁰) — `tests_symbolic::nasty_inputs_never_panic`

## 7. App (desktop UI)

- [~] *(manual)* Layout: the Paper is the page (fills everything under the header); history in a drawer from the History button, whose count pulses on a pin; tips behind ⓘ; Graph sits above the lines with a divider — built, not yet checked by hand
- [~] *(manual)* Subject menu on the ProCalc title: Math now; Physics, Engineering and Finance shown as coming soon; Math's workspaces (Paper, Geometry; Unit circle coming soon) in the same menu; remembered between launches — built, not yet checked by hand
- [~] *(manual)* Projects: New, Open, Save, Save as, Recent (Ctrl+N/O/S, Ctrl+Shift+S); `.pcmath` files that record their subject (other subjects explain themselves); unsaved dot and autosave once saved; Settings dialog with the length unit — built, not yet checked by hand
- [x] *(manual)* MathLive input with live preview while typing
- [x] *(manual)* Enter saves to history; history persists between launches
- [x] *(manual)* DEG/RAD toggle and a digits picker (16 to 1000)
- [x] *(manual)* Click a result to reuse it; Copy button
- [x] *(manual)* Symbolic results show the simplified expression only
- [x] *(manual)* History saved by v0.1 still loads (`migrateAnswer`)
- [x] The Paper evaluates a whole sheet in one pass, definitions carrying down — `tests_defs::sheets_evaluate_in_one_pass`
- [~] *(manual)* The Paper: a multi-line sheet replacing the single input, every line answered live; Enter adds a line, Backspace on an empty line removes it, ↑/↓ move between lines, Ctrl+Enter pins a line to the history — built, not yet checked by hand
- [~] *(manual)* Lines switch between maths and text notes (the number / ¶ button) — built, not yet checked by hand
- [~] *(manual)* The Paper raises when you click into it, resizes by dragging its top edge, goes full screen (⤢ or double-click the edge), and sits above MathLive's on-screen keyboard — built, not yet checked by hand

## 8. Platforms

- [x] *(manual)* Linux desktop (Linux Mint 22.3; the 0.3.0 AppImage on Ubuntu, where its updater offered 0.3.1)
- [x] *(manual)* Windows 11, updating itself through the in-app updater (0.2.0 → 0.3.0 → 0.3.1)
- [x] *(manual)* Android 9 (Galaxy Note 8): the Paper calculates, the header and toolbar fit a phone, clear of the status bar
- [~] *(manual)* Android updates: the app asks GitHub for the latest release and offers its APK, which installs over the old one — built, waits for the first signed release APK to check

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
- **`i` is always the imaginary unit**, like `e`; use a subscript (`i_1`) for a variable.
- **Variables are treated as real** when complex results are tidied into a + bi, and that
  tidying only happens without variables: (x+i)² is left as written.
- **Odd roots of negative numbers are real** (∛−8 = −2), the calculator convention, not the
  principal complex value 1 + √3i. Even roots and non-integer powers use principal values.
- **Units need upright text** (`\mathrm{m}`, `"m"` in the app) outside a conversion target,
  because a plain `m` is a variable. Upright `e` and `i` are still the constants, and `\mathrm{d}`
  is a day.
- **A degree sign followed by C or F is a temperature** (`30°C`); on its own it is an angle.
- **Temperatures add as plain numbers** within one unit: 10 °C + 5 °C = 15 °C, the way most
  calculators do it, though strictly only differences add.
- **Defined letters are values in equations:** after `x := 6`, `x = 5` reads 6 = 5 (false).
- **Answers with parameters aren't fully simplified:** ax + y = 1, x + ay = 1 gives the right
  x and y as unreduced fractions in a (common factors aren't cancelled yet).
- **The numeric root search covers [−100, 100]** (polynomials: their whole root range) and can
  miss a root where the graph touches zero without crossing.
- **Higher-degree polynomials list real roots only** when a factor has no exact formula.
- **∫ 1/x = ln|x|** and similar: the answer holds on each interval where the integrand is
  defined; the interval check before F(b) − F(a) samples 33 points, so a pole between two
  samples could slip past.
- **Integrals of degree-mode trig** carry 180/π factors, as they should, which can look surprising.
- **Results keep the units they were built from** (kg·m/s² stays that way) until converted.
- **Powers of complex numbers above 64** ((1+i)¹⁰⁰) stay as a power in the exact form; the
  decimal is still computed.
- **Definitions are eager:** `x := 3y` stores 3y as it was, and a function body uses the values
  in force when the function was defined. Changing an earlier definition doesn't update the
  results already in history.
- **Variables are one letter** (with optional subscript, `v_{max}`); only functions can have
  longer names (`area(w,h)`). User functions are called with brackets: `f(2)`, not `f 2`.
- **Integer powers are only expanded up to about 10⁶ bits** (300k digits). Larger ones stay symbolic but still get a decimal.
