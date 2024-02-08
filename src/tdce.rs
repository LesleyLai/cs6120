use std::collections::HashSet;
// Trivial Dead Code Elimination
//use crate::cfg;
use bril_rs::{Code, Function, Instruction};

// Return whether the pass converges
fn trivial_dead_code_elimination_pass(func: &mut Function) -> bool {
    let mut used = HashSet::new();

    for instr in &func.instrs {
        if let Code::Instruction(
            Instruction::Value { args, .. } | Instruction::Effect { args, .. },
        ) = instr
        {
            for arg in args {
                used.insert(arg);
            }
        }
    }

    let mut result = vec![];
    for instr in &func.instrs {
        if let Code::Instruction(
            Instruction::Constant { dest, .. } | Instruction::Value { dest, .. },
        ) = instr
        {
            if used.contains(dest) {
                result.push(instr.clone());
            }
        } else {
            result.push(instr.clone());
        }
    }

    let is_converged = func.instrs.len() == result.len();

    func.instrs = result;

    is_converged
}

pub fn trivial_dead_code_elimination(func: &mut Function) {
    while !trivial_dead_code_elimination_pass(func) {}
}
