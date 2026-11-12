fn main() {
    // DEG=1 switches to degree mode.
    let angle = if std::env::var("DEG").is_ok() { procalc_engine::AngleMode::Degrees } else { procalc_engine::AngleMode::Radians };
    // DIGITS=16 sets the precision.
    let digits = std::env::var("DIGITS").ok().and_then(|d| d.parse().ok()).unwrap_or(32);
    let st = procalc_engine::Settings { angle, digits };
    // STEPS=1 also prints the worked methods.
    let steps = std::env::var("STEPS").is_ok();
    // `-f inputs.txt` reads one input per line (shells can mangle the backslashes in \\).
    let args: Vec<String> = std::env::args().skip(1).collect();
    let inputs: Vec<String> = match args.as_slice() {
        [flag, path] if flag == "-f" => std::fs::read_to_string(path).expect("readable input file").lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect(),
        _ => args,
    };
    for c in inputs {
        match procalc_engine::evaluate(&c, &st) {
            Ok(a) => {
                println!("{c:40} | exact: {:?} | dec: {:?}", a.exact_latex, a.decimal.map(|d| d.text));
                if steps {
                    for m in &a.methods {
                        println!("  == {}", m.name);
                        for s in &m.steps {
                            println!("     {:60}  {}", s.latex, s.note);
                        }
                    }
                }
            }
            Err(e) => println!("{c:40} | ERROR: {e}"),
        }
    }
}
