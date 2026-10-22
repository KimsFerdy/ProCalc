fn main() {
    let st = procalc_engine::Settings::default();
    for c in std::env::args().skip(1) {
        match procalc_engine::evaluate(&c, &st) {
            Ok(a) => println!("{c:40} | exact: {:?} | dec: {:?}", a.exact_latex, a.decimal.map(|d| d.text)),
            Err(e) => println!("{c:40} | ERROR: {e}"),
        }
    }
}
