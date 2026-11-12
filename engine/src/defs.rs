//! User definitions: `x := 5` and `f(x) := x^2`, applied to the lines that follow.
//!
//! Definitions are notebook-style and eager. A variable stores the value its right side had
//! when it was defined, and a function body only sees the definitions made before it, so
//! nothing can refer to itself and there is no recursion to guard against.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::ast::{Expr, Stmt, name_latex};
use crate::eval::Evaluator;
use crate::sym::{Builder, SR, Sym};
use crate::{EngineError, parser};

/// Calls to user functions allowed in one evaluation. Nesting can multiply calls quickly
/// (f₂(x) := f₁(f₁(x)), f₃(x) := f₂(f₂(x)), …), so this keeps hostile input fast.
pub(crate) const MAX_CALLS: usize = 100_000;

pub struct FuncDef {
    pub params: Vec<String>,
    pub body: Expr,
    /// The definitions in force when this function was defined.
    pub scope: Rc<Env>,
}

#[derive(Default, Clone)]
pub struct Env {
    pub vars: BTreeMap<String, Sym>,
    pub funcs: BTreeMap<String, Rc<FuncDef>>,
}

impl Env {
    /// Replays earlier definition lines in order. A line that no longer evaluates
    /// (say after switching DEG/RAD) or isn't a definition is skipped.
    pub fn from_lines(lines: &[String], ev: &mut Evaluator) -> Env {
        let mut env = Env::default();
        for line in lines {
            if let Ok(Stmt::Define { name, params, body }) = parser::parse_statement(line, &env.func_names()) {
                let _ = env.define(name, params, body, ev);
            }
        }
        env
    }

    pub fn func_names(&self) -> BTreeSet<String> {
        self.funcs.keys().cloned().collect()
    }

    /// Adds one definition and returns what to show for it: the value of a variable,
    /// or the simplified body of a function.
    pub fn define(&mut self, name: String, params: Vec<String>, body: Expr, ev: &mut Evaluator) -> SR {
        // Each line gets its own call budget.
        ev.calls = 0;
        let shown = self.preview(&params, &body, ev)?;
        if params.is_empty() {
            self.vars.insert(name, shown.clone());
        } else {
            let scope = Rc::new(self.clone());
            self.funcs.insert(name, Rc::new(FuncDef { params, body, scope }));
        }
        Ok(shown)
    }

    /// The body evaluated with the parameters left as free variables.
    fn preview(&self, params: &[String], body: &Expr, ev: &mut Evaluator) -> SR {
        let mut scope = self.clone();
        for p in params {
            scope.vars.remove(p);
        }
        Ok(crate::sym::normalize_complex(Builder { ev, env: &scope }.build(body)?))
    }
}

/// Evaluates `name(args)` given the already-built arguments.
pub fn call(env: &Env, name: &str, args: Vec<Sym>, ev: &mut Evaluator) -> SR {
    let def = env
        .funcs
        .get(name)
        .ok_or_else(|| EngineError::new(format!("{} isn't defined", name_latex(name))))?;
    if args.len() != def.params.len() {
        let n = def.params.len();
        return Err(EngineError::new(format!(
            "{} takes {n} argument{}, not {}",
            name_latex(name),
            if n == 1 { "" } else { "s" },
            args.len()
        )));
    }
    ev.calls += 1;
    if ev.calls > MAX_CALLS {
        return Err(EngineError::new("Too many function calls (functions nested too deeply)"));
    }
    let mut local = (*def.scope).clone();
    for (p, a) in def.params.iter().zip(args) {
        local.vars.insert(p.clone(), a);
    }
    Builder { ev, env: &local }.build(&def.body)
}
