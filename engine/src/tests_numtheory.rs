//! Tests for number theory: gcd, lcm, mod, nCr/nPr, totient, primes, factorisation,
//! divisors, and the gamma function. Test names are referenced from FEATURES.md.

use crate::{Settings, evaluate};

fn exact(s: &str) -> String {
    let a = evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}"));
    a.exact_latex.unwrap_or_else(|| panic!("{s} has no exact form"))
}

fn dec(s: &str) -> String {
    evaluate(s, &Settings::default()).unwrap_or_else(|e| panic!("{s}: {e}")).decimal.unwrap().text
}

fn fails(s: &str) -> String {
    match evaluate(s, &Settings::default()) {
        Ok(a) => panic!("{s} should fail, gave {:?}", a.exact_latex),
        Err(e) => e.message,
    }
}

#[test]
fn gcd_and_lcm() {
    assert_eq!(exact("\\gcd(12,18)"), "6");
    assert_eq!(exact("gcd(48,18,30)"), "6");
    assert_eq!(exact("\\gcd(-12,18)"), "6");
    assert_eq!(exact("\\gcd(0,5)"), "5");
    assert_eq!(exact("\\operatorname{lcm}(4,6)"), "12");
    assert_eq!(exact("lcm(4,6,10)"), "60");
    assert_eq!(exact("\\operatorname{hcf}(84,36)"), "12");
    assert_eq!(exact("lcm(4,6)+gcd(4,6)"), "14");
    assert!(fails("gcd(x,2)").contains("letters"));
    assert!(fails("gcd(2.5,5)").contains("whole"));
}

#[test]
fn gcd_shows_euclids_algorithm() {
    let a = evaluate("\\gcd(48,18)", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Euclid's algorithm");
    let steps: Vec<&str> = a.methods[0].steps.iter().map(|s| s.latex.as_str()).collect();
    assert_eq!(steps, ["48=2\\cdot 18+12", "18=1\\cdot 12+6", "12=2\\cdot 6+0", "\\gcd=6"]);
}

#[test]
fn mod_is_never_negative() {
    assert_eq!(exact("17\\bmod5"), "2");
    assert_eq!(exact("17\\mod 5"), "2");
    assert_eq!(exact("17 mod 5"), "2");
    assert_eq!(exact("mod(17,5)"), "2");
    // a mod b = a − b⌊a/b⌋: −7 mod 3 = 2, like Python and Mathematica.
    assert_eq!(exact("-7\\bmod3"), "2");
    assert_eq!(exact("7.5\\bmod2"), "\\frac{3}{2}");
    // It binds like ×: 2 + 7 mod 3 = 2 + 1.
    assert_eq!(exact("2+7\\bmod3"), "3");
    assert!(fails("5\\bmod0").contains("undefined"));
}

#[test]
fn binomials_and_permutations() {
    assert_eq!(exact("\\binom{5}{2}"), "10");
    assert_eq!(exact("\\dbinom{10}{0}"), "1");
    assert_eq!(exact("\\binom{5}{7}"), "0");
    assert_eq!(exact("nCr(5,2)"), "10");
    assert_eq!(dec("\\binom{50}{25}"), "126410606437752");
    assert_eq!(exact("nPr(5,2)"), "20");
    assert_eq!(exact("nPr(5,0)"), "1");
    // Without a bracket, nCr is still the product n·C·r.
    assert_eq!(exact("nCr"), "Cnr");
}

#[test]
fn totient() {
    assert_eq!(exact("totient(36)"), "12");
    assert_eq!(exact("totient(97)"), "96");
    assert_eq!(exact("totient(1)"), "1");
}

#[test]
fn factor_integers_into_primes() {
    assert_eq!(exact("factor(360)"), "2^{3}\\cdot 3^{2}\\cdot 5");
    assert_eq!(dec("factor(360)"), "2^3·3^2·5");
    assert_eq!(exact("factor(12)"), "2^{2}\\cdot 3");
    assert_eq!(exact("factor(-84)"), "-2^{2}\\cdot 3\\cdot 7");
    assert_eq!(exact("factor(97)"), "97");
    // Past trial division: Pollard's rho.
    assert_eq!(dec("factor(2^{64}+1)"), "274177·67280421310721");
    assert_eq!(dec("factor(1000000007\\cdot998244353)"), "998244353·1000000007");
    // A square of a large prime.
    assert_eq!(dec("factor(1000000007^{2})"), "1000000007^2");
    // Letters are algebra, as before.
    assert_eq!(exact("factor(x^2-1)"), "\\left(x-1\\right)\\left(x+1\\right)");
    let a = evaluate("factor(360)", &Settings::default()).unwrap();
    assert_eq!(a.methods[0].name, "Prime factorisation");
}

#[test]
fn primes_and_divisors() {
    assert_eq!(dec("isprime(97)"), "prime");
    assert_eq!(dec("isprime(91)"), "not prime: 91 = 7·13");
    assert_eq!(dec("isprime(1)"), "not prime");
    assert_eq!(dec("isprime(2^{89}-1)"), "prime");
    assert_eq!(dec("isprime(2^{89}+1)").split(':').next(), Some("not prime"));
    assert_eq!(dec("divisors(12)"), "1, 2, 3, 4, 6, 12");
    assert_eq!(dec("divisors(1)"), "1");
    assert!(fails("divisors(0)").contains('0'));
    assert!(fails("isprime(2)+1").contains("own"));
}

#[test]
fn gamma_function() {
    assert_eq!(exact("\\Gamma(5)"), "24");
    assert_eq!(exact("\\Gamma\\left(\\frac{1}{2}\\right)"), "\\sqrt{\\pi}");
    assert_eq!(exact("\\Gamma(-\\frac{3}{2})"), "\\frac{4\\sqrt{\\pi}}{3}");
    assert_eq!(exact("\\Gamma(2.5)"), "\\frac{3\\sqrt{\\pi}}{4}");
    assert_eq!(exact("\\operatorname{tgamma}(5)"), "24");
    assert_eq!(dec("\\Gamma(0.3)"), "2.9915689876875906283125165159049");
    assert_eq!(dec("\\Gamma(-0.3)"), "-4.3268511088251926189372372638427");
    assert_eq!(dec("\\Gamma(\\pi)"), "2.2880377953400324179595889090602");
    // Duplication formula Γ(z)Γ(z+½) = 2^(1−2z)√π Γ(2z): different shifts, same answer.
    assert_eq!(dec("\\frac{\\Gamma(\\pi)\\Gamma(\\pi+0.5)}{2^{1-2\\pi}\\sqrt{\\pi}\\Gamma(2\\pi)}"), "1");
    assert!(fails("\\Gamma(-1)").contains("undefined"));
    assert!(fails("\\Gamma(0)").contains("undefined"));
    // Without a bracket, Γ is a Greek letter.
    assert_eq!(exact("\\Gamma+\\Gamma"), "2\\Gamma");
}

#[test]
fn factorial_of_non_integers() {
    assert_eq!(exact("2.5!"), "\\frac{15\\sqrt{\\pi}}{8}");
    assert_eq!(exact("0.5!"), "\\frac{\\sqrt{\\pi}}{2}");
    assert_eq!(dec("0.3!"), "0.89747069630627718849375495477148");
    assert_eq!(dec("\\pi!"), "7.1880827289760327020821943451248");
    assert!(fails("(-3)!").contains("negative"));
}
