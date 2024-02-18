mod cli_options;

use crate::cli_options::{parse_options, Options};
use bril_rs::{Code, ConstOps, Function, Instruction, Literal, Type, ValueOps};
use bril_utils::{instructions_to_blocks, BasicBlock};
use std::{
    collections::HashMap,
    env,
    hash::{Hash, Hasher},
};

#[derive(Debug, Clone, PartialEq)]
enum ValueExpr {
    Constant(Literal, Type),
    Op(ValueOps, Vec<String>, Type),
}

impl Hash for ValueExpr {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            ValueExpr::Constant(literal, _) => match literal {
                Literal::Int(i) => i.hash(state),
                Literal::Bool(b) => b.hash(state),
                Literal::Float(f) => f.to_bits().hash(state),
                Literal::Char(c) => c.hash(state),
            },
            ValueExpr::Op(ops, args, _) => {
                ops.hash(state);
                args.hash(state);
            }
        }
    }
}

impl Eq for ValueExpr {}

fn args_mut(instr: &mut Instruction) -> &mut [String] {
    match instr {
        Instruction::Constant { .. } => &mut [],
        Instruction::Value { args, .. } => args,
        Instruction::Effect { args, .. } => args,
    }
}

struct LVN {
    // Mapping from value tuples to canonical variables, with each row numbered
    var_and_num_from_value: HashMap<ValueExpr, (String, usize)>,
    // mapping from variable names to their current value number
    num_from_var: HashMap<String, usize>,
    canonical_var_from_num: Vec<String>,
    value_from_num: HashMap<usize, ValueExpr>,
}

impl LVN {
    fn new() -> Self {
        LVN {
            var_and_num_from_value: Default::default(),
            num_from_var: Default::default(),
            canonical_var_from_num: vec![],
            value_from_num: Default::default(),
        }
    }

    fn replace_args_with_canonical_variables(self: &mut Self, instr: &mut Instruction) {
        for arg in args_mut(instr) {
            match self.num_from_var.get(arg).copied() {
                None => {
                    // argument is not defined in this block. Create a dummy number for it
                    let num = self.canonical_var_from_num.len();
                    self.num_from_var.insert(arg.clone(), num);
                    self.canonical_var_from_num.push(arg.clone());
                }
                Some(arg_value_number) => {
                    // Use (canonical) variables in the table rather than old args
                    *arg = self.canonical_var_from_num[arg_value_number].clone();
                }
            }
        }
    }

    // Adds a new table entry with a fresh number and associate this numver with a canonical variable and a value
    fn add_fresh_num_to_table(self: &mut Self, canonical_var: String, value: ValueExpr) {
        let num = self.canonical_var_from_num.len();
        self.canonical_var_from_num.push(canonical_var.clone());
        self.value_from_num.insert(num, value.clone());
        self.var_and_num_from_value
            .insert(value, (canonical_var.clone(), num));
        self.num_from_var.insert(canonical_var, num);
    }
}

fn get_dest_and_value(instr: Instruction) -> Option<(String, ValueExpr)> {
    match instr {
        Instruction::Constant {
            dest,
            value,
            const_type,
            ..
        } => {
            let value = ValueExpr::Constant(value, const_type);
            Some((dest, value))
        }
        Instruction::Value {
            op,
            args,
            dest,
            op_type,
            ..
        } => {
            let value = ValueExpr::Op(op, args.clone(), op_type);
            Some((dest, value))
        }
        Instruction::Effect { .. } => None,
    }
}

fn lvn_block_pass(block: &mut BasicBlock, option: &Options) {
    let mut lvn = LVN::new();

    for code in block {
        if let Code::Instruction(instr) = code {
            match get_dest_and_value(instr.clone()) {
                None => {
                    // Effects
                    lvn.replace_args_with_canonical_variables(instr);
                }
                Some((dest, value)) => {
                    let num: usize;

                    // Already in table
                    match lvn.var_and_num_from_value.get(&value.clone()) {
                        Some((var, num2)) => {
                            // This value have been computed before. Reuse it
                            num = *num2;

                            *code = Code::Instruction(Instruction::Value {
                                args: vec![var.clone()],
                                dest: dest.clone(),
                                funcs: vec![],
                                labels: vec![],
                                op: ValueOps::Id,
                                pos: None,
                                // TODO: more types
                                op_type: Type::Int,
                            });

                            lvn.num_from_var.insert(dest.clone(), num);
                        }
                        // A brand-new value
                        None => {
                            if option.handle_copy_propagate {
                                match value {
                                    ValueExpr::Op(ValueOps::Id, args, typ) => {
                                        match lvn.num_from_var.get(&args[0]) {
                                            None => {
                                                // argument is not defined in this block. Create a dummy number for it
                                                let num = lvn.canonical_var_from_num.len();
                                                lvn.num_from_var.insert(args[0].clone(), num);
                                                lvn.canonical_var_from_num.push(args[0].clone());
                                                lvn.num_from_var.insert(dest.clone(), num);
                                            }
                                            // If arg already has associate number
                                            Some(num) => {
                                                // check whether the original is a constant
                                                match &lvn.value_from_num.get(num) {
                                                    Some(ValueExpr::Constant(literal, typ)) => {
                                                        *code = Code::Instruction(
                                                            Instruction::Constant {
                                                                dest: dest.clone(),
                                                                op: ConstOps::Const,
                                                                pos: None,
                                                                const_type: typ.clone(),
                                                                value: literal.clone(),
                                                            },
                                                        )
                                                    }
                                                    _ => {
                                                        *code =
                                                            Code::Instruction(Instruction::Value {
                                                                args: vec![lvn
                                                                    .canonical_var_from_num[*num]
                                                                    .clone()],
                                                                dest: dest.clone(),
                                                                funcs: vec![],
                                                                labels: vec![],
                                                                op: ValueOps::Id,
                                                                pos: None,
                                                                op_type: typ.clone(),
                                                            })
                                                    }
                                                };

                                                lvn.num_from_var.insert(dest.clone(), *num);
                                            }
                                        }

                                        continue;
                                    }
                                    _ => {}
                                }
                            }

                            // A newly computed value.
                            lvn.add_fresh_num_to_table(dest.clone(), value.clone());
                            lvn.replace_args_with_canonical_variables(instr);
                        }
                    }
                }
            }
        }
    }
}

fn local_value_numbering(func: &mut Function, option: &Options) {
    let mut blocks = instructions_to_blocks(&func.instrs);

    for block in &mut blocks {
        lvn_block_pass(block, option);
    }

    func.instrs = vec![];
    for block in &mut blocks {
        func.instrs.append(block);
    }
}

fn main() {
    let args: Box<[String]> = env::args().collect::<Vec<_>>().into_boxed_slice();
    let options = parse_options(&args);

    let mut program = bril_rs::load_program();

    for function in &mut program.functions {
        local_value_numbering(function, &options);
    }

    bril_rs::output_program(&program);
}
