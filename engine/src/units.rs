//! Physical units: SI and common non-SI units with prefixes, dimension checking, conversion,
//! and temperatures with offsets (°C, °F).
//!
//! Units are atoms ([`Sym::Unit`]) in the symbolic tree, so 5 kg + 3 kg = 8 kg and m·m = m²
//! come from the ordinary simplifier. [`resolve`] then checks dimensions and writes the result
//! as one unit-free value times one product of units.
//!
//! Temperatures: a value in plain °C or °F converts with the offset (20 °C → 68 °F). Inside
//! compound units (J/(kg·°C)) they act as temperature differences, where 1 °C = 1 K.

use std::collections::BTreeMap;

use num_bigint::BigInt;
use num_rational::BigRational as Q;
use num_traits::{One, Signed, ToPrimitive, Zero};

use crate::EngineError;
use crate::number;
use crate::sym::{self, SR, Sym, add, mul, n};

/// Exponents of the SI base dimensions, in the order m, kg, s, A, K, mol, cd.
type Dims = [Q; 7];

struct Base {
    name: &'static str,
    dims: [i8; 7],
    /// Size in SI base units, as an exact decimal or quotient of decimals.
    scale: &'static str,
    prefixes: bool,
    /// For °C and °F: absolute zero measured in the unit, negated (273.15 for °C), so that
    /// kelvin = (x + zero)·scale.
    zero: Option<&'static str>,
}

const fn si(name: &'static str, dims: [i8; 7], scale: &'static str) -> Base {
    Base { name, dims, scale, prefixes: true, zero: None }
}

const fn other(name: &'static str, dims: [i8; 7], scale: &'static str) -> Base {
    Base { name, dims, scale, prefixes: false, zero: None }
}

const LENGTH: [i8; 7] = [1, 0, 0, 0, 0, 0, 0];
const MASS: [i8; 7] = [0, 1, 0, 0, 0, 0, 0];
const TIME: [i8; 7] = [0, 0, 1, 0, 0, 0, 0];
const TEMP: [i8; 7] = [0, 0, 0, 0, 1, 0, 0];
const AREA: [i8; 7] = [2, 0, 0, 0, 0, 0, 0];
const VOLUME: [i8; 7] = [3, 0, 0, 0, 0, 0, 0];
const SPEED: [i8; 7] = [1, 0, -1, 0, 0, 0, 0];
const FORCE: [i8; 7] = [1, 1, -2, 0, 0, 0, 0];
const PRESSURE: [i8; 7] = [-1, 1, -2, 0, 0, 0, 0];
const ENERGY: [i8; 7] = [2, 1, -2, 0, 0, 0, 0];
const POWER: [i8; 7] = [2, 1, -3, 0, 0, 0, 0];

const BASES: &[Base] = &[
    // SI base units (the kilogram is built from the gram so prefixes work).
    si("m", LENGTH, "1"),
    si("g", MASS, "0.001"),
    si("s", TIME, "1"),
    si("A", [0, 0, 0, 1, 0, 0, 0], "1"),
    si("K", TEMP, "1"),
    si("mol", [0, 0, 0, 0, 0, 1, 0], "1"),
    si("cd", [0, 0, 0, 0, 0, 0, 1], "1"),
    // SI derived units.
    si("Hz", [0, 0, -1, 0, 0, 0, 0], "1"),
    si("N", FORCE, "1"),
    si("Pa", PRESSURE, "1"),
    si("J", ENERGY, "1"),
    si("W", POWER, "1"),
    si("C", [0, 0, 1, 1, 0, 0, 0], "1"),
    si("V", [2, 1, -3, -1, 0, 0, 0], "1"),
    si("Ω", [2, 1, -3, -2, 0, 0, 0], "1"),
    si("F", [-2, -1, 4, 2, 0, 0, 0], "1"),
    si("H", [2, 1, -2, -2, 0, 0, 0], "1"),
    si("S", [-2, -1, 3, 2, 0, 0, 0], "1"),
    si("T", [0, 1, -2, -1, 0, 0, 0], "1"),
    si("Wb", [2, 1, -2, -1, 0, 0, 0], "1"),
    si("L", VOLUME, "0.001"),
    si("t", MASS, "1000"),
    si("bar", PRESSURE, "100000"),
    si("eV", ENERGY, "1.602176634e-19"),
    si("cal", ENERGY, "4.184"),
    si("Wh", ENERGY, "3600"),
    si("Da", MASS, "1.66053906660e-27"),
    // Temperatures with an offset.
    Base { name: "°C", dims: TEMP, scale: "1", prefixes: false, zero: Some("273.15") },
    Base { name: "°F", dims: TEMP, scale: "5/9", prefixes: false, zero: Some("459.67") },
    // Time.
    other("min", TIME, "60"),
    other("h", TIME, "3600"),
    other("d", TIME, "86400"),
    other("wk", TIME, "604800"),
    other("yr", TIME, "31557600"),
    // Imperial and other lengths, areas, volumes, masses.
    other("in", LENGTH, "0.0254"),
    other("ft", LENGTH, "0.3048"),
    other("yd", LENGTH, "0.9144"),
    other("mi", LENGTH, "1609.344"),
    other("nmi", LENGTH, "1852"),
    other("au", LENGTH, "149597870700"),
    other("ly", LENGTH, "9460730472580800"),
    other("ha", AREA, "10000"),
    other("acre", AREA, "4046.8564224"),
    other("gal", VOLUME, "0.003785411784"),
    other("lb", MASS, "0.45359237"),
    other("oz", MASS, "0.028349523125"),
    other("u", MASS, "1.66053906660e-27"),
    // Speed, force, pressure, energy, power.
    other("mph", SPEED, "0.44704"),
    other("kn", SPEED, "1852/3600"),
    other("lbf", FORCE, "4.4482216152605"),
    other("atm", PRESSURE, "101325"),
    other("psi", PRESSURE, "4.4482216152605/0.00064516"),
    other("mmHg", PRESSURE, "133.322387415"),
    other("Torr", PRESSURE, "101325/760"),
    other("BTU", ENERGY, "1055.05585262"),
    other("hp", POWER, "745.69987158227022"),
];

/// Longer prefixes first, so "da" wins over "d".
const PREFIXES: &[(&str, i32)] = &[
    ("da", 1),
    ("Q", 30),
    ("R", 27),
    ("Y", 24),
    ("Z", 21),
    ("E", 18),
    ("P", 15),
    ("T", 12),
    ("G", 9),
    ("M", 6),
    ("k", 3),
    ("h", 2),
    ("d", -1),
    ("c", -2),
    ("m", -3),
    ("µ", -6),
    ("u", -6),
    ("n", -9),
    ("p", -12),
    ("f", -15),
    ("a", -18),
    ("z", -21),
    ("y", -24),
    ("r", -27),
    ("q", -30),
];

/// Other spellings of unit names.
fn alias(name: &str) -> &str {
    match name {
        "l" => "L",
        "ohm" => "Ω",
        "degC" | "celsius" | "℃" => "°C",
        "degF" | "fahrenheit" | "℉" => "°F",
        "sec" => "s",
        other => other,
    }
}

fn base(name: &str) -> Option<&'static Base> {
    BASES.iter().find(|b| b.name == name)
}

/// The canonical name of a unit (`km`, `µm`, `kΩ`, `°C`), or None if it isn't one.
pub fn canonical(name: &str) -> Option<String> {
    let name = alias(name);
    if base(name).is_some() {
        return Some(name.to_string());
    }
    for (p, _) in PREFIXES {
        if let Some(rest) = name.strip_prefix(p)
            && let Some(b) = base(alias(rest)).filter(|b| b.prefixes)
        {
            let p = if *p == "u" { "µ" } else { p };
            return Some(format!("{p}{}", b.name));
        }
    }
    None
}

struct UnitDef {
    dims: [i8; 7],
    scale: Q,
    zero: Option<Q>,
}

fn parse_decimal(s: &str) -> Q {
    let (mant, exp) = s.split_once('e').unwrap_or((s, "0"));
    let m = number::parse_exact(mant).expect("unit table holds valid numbers");
    let e: i32 = exp.parse().expect("unit table holds valid exponents");
    let ten = Q::from_integer(BigInt::from(10));
    m * num_traits::pow::Pow::pow(&ten, e)
}

fn parse_scale(s: &str) -> Q {
    match s.split_once('/') {
        Some((a, b)) => parse_decimal(a) / parse_decimal(b),
        None => parse_decimal(s),
    }
}

/// Looks up a canonical unit name.
fn def(name: &str) -> UnitDef {
    let (b, k) = match base(name) {
        Some(b) => (b, 0),
        None => PREFIXES
            .iter()
            .find_map(|(p, k)| name.strip_prefix(p).and_then(base).map(|b| (b, *k)))
            .expect("unit names are checked by the parser"),
    };
    let ten = Q::from_integer(BigInt::from(10));
    UnitDef {
        dims: b.dims,
        scale: parse_scale(b.scale) * num_traits::pow::Pow::pow(&ten, k),
        zero: b.zero.map(parse_decimal),
    }
}

// ---------- Display ----------

/// `\mathrm{km}`, `\mu\mathrm{m}`, `\mathrm{k}\Omega`, `{}^{\circ}\mathrm{C}`
pub fn unit_latex(name: &str) -> String {
    if let Some(rest) = name.strip_prefix('°') {
        return format!("{{}}^{{\\circ}}\\mathrm{{{rest}}}");
    }
    let (head, omega) = match name.strip_suffix('Ω') {
        Some(h) => (h, true),
        None => (name, false),
    };
    let (micro, head) = match head.strip_prefix('µ') {
        Some(h) => (true, h),
        None => (false, head),
    };
    let mut out = String::new();
    if micro {
        out.push_str("\\mu ");
    }
    if !head.is_empty() {
        out.push_str(&format!("\\mathrm{{{head}}}"));
    }
    if omega {
        out.push_str("\\Omega ");
    }
    out.trim_end().to_string()
}

fn exponent_latex(e: &Q) -> String {
    if e.is_integer() { e.to_string() } else { format!("\\frac{{{}}}{{{}}}", e.numer(), e.denom()) }
}

/// Units as (LaTeX, plain text): `\mathrm{m}/\mathrm{s}^{2}` and `m/s^2`.
pub fn display(units: &BTreeMap<String, Q>) -> (String, String) {
    let part = |ups: Vec<(&String, Q)>| -> (String, String) {
        let latex: Vec<String> = ups
            .iter()
            .map(|(u, e)| {
                if e.is_one() { unit_latex(u) } else { format!("{}^{{{}}}", unit_latex(u), exponent_latex(e)) }
            })
            .collect();
        let text: Vec<String> =
            ups.iter().map(|(u, e)| if e.is_one() { u.to_string() } else { format!("{u}^{e}") }).collect();
        (latex.join("\\,"), text.join("·"))
    };
    let top: Vec<_> = units.iter().filter(|(_, e)| e.is_positive()).map(|(u, e)| (u, e.clone())).collect();
    let bottom: Vec<_> = units.iter().filter(|(_, e)| e.is_negative()).map(|(u, e)| (u, -e)).collect();
    let several = bottom.len() > 1;
    let (tl, tt) = part(top);
    if bottom.is_empty() {
        return (tl, tt);
    }
    let (bl, bt) = part(bottom);
    let (bl, bt) = if several { (format!("\\left({bl}\\right)"), format!("({bt})")) } else { (bl, bt) };
    let (tl, tt) = if tl.is_empty() { ("1".to_string(), "1".to_string()) } else { (tl, tt) };
    (format!("{tl}/{bl}"), format!("{tt}/{bt}"))
}

fn units_text(units: &BTreeMap<String, Q>) -> String {
    if units.is_empty() { "a plain number".into() } else { display(units).1 }
}

// ---------- Quantities ----------

pub fn contains_unit(s: &Sym) -> bool {
    match s {
        Sym::Unit(_) => true,
        Sym::Num(_) | Sym::Const(_) | Sym::Var(_) => false,
        Sym::Add(v) | Sym::Mul(v) => v.iter().any(contains_unit),
        Sym::Pow(a, b) | Sym::Log(a, b) => contains_unit(a) || contains_unit(b),
        Sym::Func(_, a) | Sym::Factorial(a, _) => contains_unit(a),
        Sym::Matrix(m) => m.iter().flatten().any(contains_unit),
        Sym::Integral { body, lo, hi, .. } | Sym::Sum { body, lo, hi, .. } => contains_unit(body) || contains_unit(lo) || contains_unit(hi),
    }
}

/// Just the units of an expression (5 m/s → m/s), if it has any and they make sense.
pub fn unit_part(s: &Sym) -> Option<Sym> {
    if !contains_unit(s) {
        return None;
    }
    let q = resolve(s).ok()?;
    (!q.units.is_empty()).then(|| q.unit_sym())
}

fn powq(b: Sym, e: &Q) -> Sym {
    sym::pow(b.clone(), Sym::Num(e.clone())).unwrap_or_else(|_| Sym::Pow(Box::new(b), Box::new(Sym::Num(e.clone()))))
}

/// A unit-free value times a product of units, each with its exponent.
pub struct Quantity {
    pub value: Sym,
    pub units: BTreeMap<String, Q>,
}

impl Quantity {
    fn plain(value: Sym) -> Quantity {
        Quantity { value, units: BTreeMap::new() }
    }

    fn dims(&self) -> Dims {
        let mut d: Dims = std::array::from_fn(|_| Q::zero());
        for (u, e) in &self.units {
            for (slot, k) in d.iter_mut().zip(def(u).dims) {
                *slot += e * Q::from_integer(k.into());
            }
        }
        d
    }

    /// Size of the units in SI base units, exactly (√km is a surd).
    fn scale(&self) -> Sym {
        mul(self.units.iter().map(|(u, e)| powq(Sym::Num(def(u).scale), e)).collect())
    }

    /// log₁₀ of the size, for choosing the largest unit.
    fn log_scale(&self) -> f64 {
        self.units.iter().map(|(u, e)| log10(&def(u).scale) * e.to_f64().unwrap_or(0.0)).sum()
    }

    fn unit_sym(&self) -> Sym {
        mul(self.units.iter().map(|(u, e)| powq(Sym::Unit(u.clone()), e)).collect())
    }
}

fn log10(q: &Q) -> f64 {
    q.to_f64().map(f64::log10).unwrap_or(0.0)
}

fn ratio(a: Sym, b: Sym) -> Sym {
    sym::div(a, b).expect("unit scales are never zero")
}

/// Splits an expression into a value and units, combining the terms of sums.
fn split(s: &Sym) -> Result<Quantity, EngineError> {
    if !contains_unit(s) {
        return Ok(Quantity::plain(s.clone()));
    }
    match s {
        Sym::Unit(u) => Ok(Quantity { value: n(1), units: BTreeMap::from([(u.clone(), Q::one())]) }),
        Sym::Mul(fs) => {
            let mut values = Vec::new();
            let mut units: BTreeMap<String, Q> = BTreeMap::new();
            for f in fs {
                let q = split(f)?;
                values.push(q.value);
                for (u, e) in q.units {
                    *units.entry(u).or_insert_with(Q::zero) += e;
                }
            }
            units.retain(|_, e| !e.is_zero());
            Ok(Quantity { value: mul(values), units })
        }
        Sym::Pow(b, e) => {
            if contains_unit(e) {
                return Err(EngineError::new("An exponent can't have units"));
            }
            let Some(k) = e.as_num() else {
                return Err(EngineError::new("Units can only be raised to a number"));
            };
            let q = split(b)?;
            Ok(Quantity { value: powq(q.value, k), units: q.units.into_iter().map(|(u, x)| (u, x * k)).collect() })
        }
        Sym::Add(ts) => combine(ts.iter().map(split).collect::<Result<_, _>>()?),
        Sym::Func(crate::ast::Func::Abs, x) => {
            // Units are positive, so |−5 m| = 5 m.
            let q = resolve(x)?;
            Ok(Quantity { value: Sym::Func(crate::ast::Func::Abs, Box::new(q.value)), units: q.units })
        }
        Sym::Func(f, x) => {
            let q = resolve(x)?;
            if !q.units.is_empty() {
                return Err(EngineError::new(format!(
                    "{} needs a plain number, not {}",
                    f.latex().trim_start_matches('\\').trim_start_matches("operatorname{").trim_end_matches('}'),
                    units_text(&q.units)
                )));
            }
            Ok(Quantity::plain(Sym::Func(*f, Box::new(q.value))))
        }
        _ => Err(EngineError::new("Units can't go inside logarithms with a base or factorials")),
    }
}

fn has_offset(units: &BTreeMap<String, Q>) -> bool {
    units.keys().any(|u| def(u).zero.is_some())
}

/// Adds quantities: the units must have the same dimension, and the result uses the largest.
fn combine(qs: Vec<Quantity>) -> Result<Quantity, EngineError> {
    let first = &qs[0];
    if qs.iter().all(|q| q.units == first.units) {
        let units = first.units.clone();
        return Ok(Quantity { value: add(qs.into_iter().map(|q| q.value).collect()), units });
    }
    let d0 = first.dims();
    for q in &qs[1..] {
        if q.dims() != d0 {
            return Err(EngineError::new(format!(
                "Can't add {} and {}: they measure different things",
                units_text(&first.units),
                units_text(&q.units)
            )));
        }
    }
    if qs.iter().any(|q| has_offset(&q.units)) {
        return Err(EngineError::new(
            "Adding °C, °F and K together is ambiguous: convert to one of them first",
        ));
    }
    let target = qs
        .iter()
        .max_by(|a, b| a.log_scale().total_cmp(&b.log_scale()))
        .map(|q| Quantity { value: n(1), units: q.units.clone() })
        .expect("a sum has terms");
    let ts = target.scale();
    let value = add(qs.into_iter().map(|q| mul(vec![q.value.clone(), ratio(q.scale(), ts.clone())])).collect());
    Ok(Quantity { value, units: target.units })
}

/// Units of the same dimension in one product become one (km·m = 0.001 km²), and units that
/// cancel to a plain number disappear (L/cm³ = 1000).
fn tidy(q: Quantity) -> Quantity {
    let Quantity { mut value, mut units } = q;
    let mut groups: BTreeMap<[i8; 7], Vec<String>> = BTreeMap::new();
    for u in units.keys() {
        groups.entry(def(u).dims).or_default().push(u.clone());
    }
    for (_, names) in groups.into_iter().filter(|(_, v)| v.len() > 1) {
        let keep = names
            .iter()
            .max_by(|a, b| log10(&def(a).scale).total_cmp(&log10(&def(b).scale)))
            .cloned()
            .expect("group is non-empty");
        let keep_scale = def(&keep).scale;
        for u in names.into_iter().filter(|u| *u != keep) {
            let e = units.remove(&u).expect("unit is in the map");
            value = mul(vec![value, powq(Sym::Num(def(&u).scale / &keep_scale), &e)]);
            *units.entry(keep.clone()).or_insert_with(Q::zero) += e;
        }
    }
    units.retain(|_, e| !e.is_zero());
    let q = Quantity { value, units };
    if !q.units.is_empty() && q.dims().iter().all(Q::is_zero) {
        let scale = q.scale();
        return Quantity::plain(mul(vec![q.value, scale]));
    }
    q
}

/// The expression as a single value times a single product of units.
pub fn resolve(s: &Sym) -> Result<Quantity, EngineError> {
    Ok(tidy(split(s)?))
}

/// A lone °C, °F or K (power 1), which converts with offsets.
fn bare_temperature(units: &BTreeMap<String, Q>) -> Option<UnitDef> {
    let (u, e) = units.iter().next().filter(|_| units.len() == 1)?;
    let d = def(u);
    (e.is_one() && d.dims == TEMP && (d.zero.is_some() || u == "K")).then_some(d)
}

/// `x → target`: the value of x expressed in the target units.
pub fn convert(x: Sym, target: Sym) -> SR {
    let q = resolve(&x)?;
    let t = split(&target)?;
    if !t.value.is_one() || t.units.is_empty() {
        return Err(EngineError::new("Convert to units only, as in → km/h"));
    }
    if q.dims() != t.dims() {
        return Err(EngineError::new(format!(
            "Can't convert {} to {}: they measure different things",
            units_text(&q.units),
            units_text(&t.units)
        )));
    }
    let value = match (bare_temperature(&q.units), bare_temperature(&t.units)) {
        (Some(from), Some(to)) => {
            // kelvin = (x + zero)·scale, then back out in the target unit.
            let zero = |d: &UnitDef| Sym::Num(d.zero.clone().unwrap_or_else(Q::zero));
            let kelvin = mul(vec![add(vec![q.value, zero(&from)]), Sym::Num(from.scale)]);
            add(vec![ratio(kelvin, Sym::Num(to.scale.clone())), sym::neg(zero(&to))])
        }
        _ => mul(vec![q.value.clone(), ratio(q.scale(), t.scale())]),
    };
    Ok(mul(vec![value, t.unit_sym()]))
}
