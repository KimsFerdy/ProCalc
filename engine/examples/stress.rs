//! Stress test: runs every line of `stress/corpus.txt` through the engine and reports
//! wrong answers, unexpected errors, panics, hangs and slow cases.
//!
//! ```text
//! cargo run -p procalc-engine --example stress            # whole corpus
//! cargo run -p procalc-engine --example stress -- trig    # sections whose name contains "trig"
//! ```
//!
//! Corpus format, one case per line:
//!
//! ```text
//! ## Section name           starts a section (and forgets definitions)
//! @deg  @rad  @digits 16    settings for the rest of the section
//! input                     just run it: must not crash, hang or error
//! input => = latex          the exact answer must be this LaTeX
//! input => ~ text           the decimal answer must be this text
//! input => !                must be an error
//! input => ! words          must be an error whose message contains `words`
//! input => todo             not built yet: expected to fail, reported if it starts working
//! # comment
//! ```
//!
//! A definition line (`x := 5`) that runs without error is kept for the rest of the section.
//! The full listing goes to `stress/report.md`.

use std::fmt::Write as _;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use procalc_engine::{AngleMode, Answer, EngineError, Settings, evaluate_with};

const SLOW: Duration = Duration::from_secs(2);
const HANG: Duration = Duration::from_secs(20);

enum Expect {
    Run,
    Exact(String),
    Decimal(String),
    Error(String),
    Todo,
}

#[derive(PartialEq)]
enum Verdict {
    Pass,
    Wrong,
    Panic,
    Hang,
    TodoStillTodo,
    TodoNowWorks,
}

fn parse_expect(s: &str) -> Expect {
    let s = s.trim();
    if s == "todo" {
        Expect::Todo
    } else if let Some(rest) = s.strip_prefix('=') {
        Expect::Exact(rest.trim().to_string())
    } else if let Some(rest) = s.strip_prefix('~') {
        Expect::Decimal(rest.trim().to_string())
    } else if let Some(rest) = s.strip_prefix('!') {
        Expect::Error(rest.trim().to_string())
    } else {
        Expect::Run
    }
}

/// Runs one case on its own thread so a panic or a hang can't take the run down.
fn run_case(input: &str, defs: &[String], settings: &Settings) -> (Option<Result<Answer, EngineError>>, Duration) {
    let (tx, rx) = mpsc::channel();
    let (input, defs, settings) = (input.to_string(), defs.to_vec(), settings.clone());
    let start = Instant::now();
    std::thread::spawn(move || {
        let r = std::panic::catch_unwind(|| evaluate_with(&input, &defs, &settings));
        let _ = tx.send(r);
    });
    match rx.recv_timeout(HANG) {
        Ok(Ok(r)) => (Some(r), start.elapsed()),
        Ok(Err(_panic)) => (None, start.elapsed()),
        Err(_) => (None, HANG),
    }
}

fn shown(r: &Result<Answer, EngineError>) -> String {
    match r {
        Ok(a) => {
            let mut parts = Vec::new();
            if let Some(e) = &a.exact_latex {
                parts.push(format!("= `{e}`"));
            }
            if let Some(d) = &a.decimal {
                parts.push(format!("~ `{}`", d.text));
            }
            if parts.is_empty() { "(empty answer)".into() } else { parts.join("  ") }
        }
        Err(e) => format!("ERROR: {}", e.message),
    }
}

fn judge(expect: &Expect, r: &Result<Answer, EngineError>) -> Verdict {
    let ok = match (expect, r) {
        (Expect::Todo, Ok(_)) => return Verdict::TodoNowWorks,
        (Expect::Todo, Err(_)) => return Verdict::TodoStillTodo,
        (Expect::Run, r) => r.is_ok(),
        (Expect::Exact(want), Ok(a)) => a.exact_latex.as_deref() == Some(want.as_str()),
        (Expect::Decimal(want), Ok(a)) => a.decimal.as_ref().map(|d| d.text.as_str()) == Some(want.as_str()),
        (Expect::Error(words), Err(e)) => e.message.contains(words.as_str()),
        _ => false,
    };
    if ok { Verdict::Pass } else { Verdict::Wrong }
}

fn main() {
    let filter = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../stress");
    let corpus = std::fs::read_to_string(format!("{root}/corpus.txt")).expect("stress/corpus.txt");

    let mut settings = Settings::default();
    let mut defs: Vec<String> = Vec::new();
    let mut section = String::from("(start)");
    let mut report = String::from("# ProCalc stress report\n\n");
    let mut problems = String::new();
    let (mut total, mut passed, mut wrong, mut crashed, mut slow, mut todo, mut now_works) = (0, 0, 0, 0, 0, 0, 0);
    let run_start = Instant::now();

    for (lineno, raw) in corpus.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') && !line.starts_with("##") {
            continue;
        }
        if let Some(name) = line.strip_prefix("##") {
            section = name.trim().to_string();
            settings = Settings::default();
            defs.clear();
            if section.to_lowercase().contains(&filter) {
                let _ = writeln!(report, "\n## {section}\n\n| line | input | result | ms | |\n|---|---|---|---|---|");
            }
            continue;
        }
        if !section.to_lowercase().contains(&filter) {
            continue;
        }
        match line {
            "@deg" => settings.angle = AngleMode::Degrees,
            "@rad" => settings.angle = AngleMode::Radians,
            _ if line.starts_with("@digits") => {
                settings.digits = line[7..].trim().parse().expect("@digits takes a number");
            }
            _ => {
                let (input, expect) = match line.split_once("=>") {
                    Some((i, e)) => (i.trim(), parse_expect(e)),
                    None => (line, Expect::Run),
                };
                total += 1;
                let (result, took) = run_case(input, &defs, &settings);
                let ms = took.as_millis();
                let (verdict, out) = match &result {
                    None if took >= HANG => (Verdict::Hang, "HANG (gave up waiting)".to_string()),
                    None => (Verdict::Panic, "PANIC".to_string()),
                    Some(r) => (judge(&expect, r), shown(r)),
                };
                if let Some(Ok(a)) = &result
                    && a.defines.is_some()
                {
                    defs.push(input.to_string());
                }
                let mark = match verdict {
                    Verdict::Pass => {
                        passed += 1;
                        "ok"
                    }
                    Verdict::TodoStillTodo => {
                        todo += 1;
                        "todo"
                    }
                    Verdict::TodoNowWorks => {
                        now_works += 1;
                        "NOW WORKS"
                    }
                    Verdict::Wrong => {
                        wrong += 1;
                        "WRONG"
                    }
                    Verdict::Panic | Verdict::Hang => {
                        crashed += 1;
                        "CRASH"
                    }
                };
                let is_slow = took >= SLOW;
                if is_slow {
                    slow += 1;
                }
                let flag = if is_slow { format!("{mark} SLOW") } else { mark.to_string() };
                let cell = |s: &str| s.replace('|', "\\|");
                let _ = writeln!(report, "| {} | `{}` | {} | {ms} | {flag} |", lineno + 1, cell(input), cell(&out));
                if !matches!(verdict, Verdict::Pass | Verdict::TodoStillTodo) || is_slow {
                    let want = match &expect {
                        Expect::Exact(w) => format!("  (wanted = {w})"),
                        Expect::Decimal(w) => format!("  (wanted ~ {w})"),
                        Expect::Error(w) => format!("  (wanted an error{})", if w.is_empty() { String::new() } else { format!(" with \"{w}\"") }),
                        _ => String::new(),
                    };
                    let _ = writeln!(problems, "  [{flag}] line {}  {section}\n      {input}\n      -> {out}{want}  ({ms} ms)", lineno + 1);
                }
            }
        }
    }

    let summary = format!(
        "{total} cases in {:.1}s: {passed} ok, {wrong} wrong, {crashed} crashed/hung, {slow} slow, \
         {todo} todo, {now_works} todo now working",
        run_start.elapsed().as_secs_f64()
    );
    let _ = writeln!(report, "\n---\n\n{summary}\n");
    std::fs::write(format!("{root}/report.md"), &report).expect("write stress/report.md");
    if !problems.is_empty() {
        println!("Problems:\n{problems}");
    }
    println!("{summary}\nFull listing: stress/report.md");
    // Threads still stuck on a hang would keep the process alive.
    std::process::exit(if wrong + crashed > 0 { 1 } else { 0 });
}
