mod cli_options;

use crate::cli_options::{parse_options, Options};
use bril_rs::{Code, ConstOps, Function, Instruction, Literal, ValueOps};
use bril_utils::{instructions_to_blocks, BasicBlock};
use std::{
    collections::HashMap,
    env,
    hash::{Hash, Hasher},
};

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Constant(Literal),
    Op(ValueOps, Vec<String>),
}

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Constant(literal) => match literal {
                Literal::Int(i) => i.hash(state),
                Literal::Bool(b) => b.hash(state),
                Literal::Float(f) => f.to_bits().hash(state),
                Literal::Char(c) => c.hash(state),
            },
            Value::Op(ops, args) => {
                ops.hash(state);
                args.hash(state);
            }
        }
    }
}

impl Eq for Value {}

fn args_mut(instr: &mut Instruction) -> &mut [String] {
    match instr {
        Instruction::Constant { .. } => &mut [],
        Instruction::Value { args, .. } => args,
        Instruction::Effect { args, .. } => args,
    }
}

fn lvn_block_pass(block: &mut BasicBlock, option: &Options) {
    // Mapping from value tuples to canonical variables, with each row numbered
    let mut var_from_value: HashMap<Value, (String, usize)> = HashMap::new();
    // mapping from variable names to their current value number
    let mut num_from_var: HashMap<String, usize> = HashMap::new();
    let mut canonical_var_from_num: Vec<String> = vec![];
    let mut value_from_num: Vec<Value> = vec![];

    for code in block {
        if let Code::Instruction(instr) = code {
            let maybe_dest_value_pair = match instr.clone() {
                Instruction::Constant { dest, value, .. } => {
                    let value = Value::Constant(value);
                    Some((dest, value))
                }
                Instruction::Value { op, args, dest, .. } => {
                    let value = Value::Op(op, args.clone());
                    Some((dest, value))
                }
                Instruction::Effect { .. } => None,
            };

            match maybe_dest_value_pair {
                None => {
                    // Effects
                    for arg in args_mut(instr) {
                        // Argument may not have a value number if it was defined in another basic block
                        if let Some(&arg_value_number) = num_from_var.get(arg) {
                            *arg = canonical_var_from_num[arg_value_number].clone();
                        }
                    }
                }
                Some((dest, value)) => {
                    let num: usize;

                    // Already in table
                    if var_from_value.contains_key(&value.clone()) {
                        // This value have been computed before. Reuse it
                        let (var, num2) = var_from_value.get(&value.clone()).unwrap();
                        num = *num2;

                        *code = Code::Instruction(Instruction::Value {
                            args: vec![var.clone()],
                            dest: dest.clone(),
                            funcs: vec![],
                            labels: vec![],
                            op: ValueOps::Id,
                            pos: instr.get_pos(),
                            // TODO: more types
                            op_type: bril_rs::Type::Int,
                        });
                    } else {
                        if option.handle_copy_propagate {
                            match value {
                                Value::Op(ValueOps::Id, args) => {
                                    match num_from_var.get(&args[0]) {
                                        None => {}
                                        // If arg already has associate number
                                        Some(num) => {
                                            // TODO: check whether the original is a constant
                                            match &value_from_num[*num] {
                                                Value::Constant(literal) => {
                                                    *code =
                                                        Code::Instruction(Instruction::Constant {
                                                            dest: dest.clone(),
                                                            op: ConstOps::Const,
                                                            pos: instr.get_pos(),
                                                            // TODO: more types
                                                            const_type: bril_rs::Type::Int,
                                                            value: literal.clone(),
                                                        })
                                                }
                                                _ => {
                                                    *code = Code::Instruction(Instruction::Value {
                                                        args: vec![
                                                            canonical_var_from_num[*num].clone()
                                                        ],
                                                        dest: dest.clone(),
                                                        funcs: vec![],
                                                        labels: vec![],
                                                        op: ValueOps::Id,
                                                        pos: instr.get_pos(),
                                                        // TODO: more types
                                                        op_type: bril_rs::Type::Int,
                                                    })
                                                }
                                            };

                                            num_from_var.insert(dest.clone(), *num);
                                        }
                                    }

                                    continue;
                                }
                                _ => {}
                            }
                        }

                        // A newly computed value.
                        num = canonical_var_from_num.len();
                        canonical_var_from_num.push(dest.clone());
                        value_from_num.push(value.clone());

                        // Add to table
                        var_from_value.insert(value.clone(), (dest.clone(), num));

                        // Use (canonical) variables in the table rather than old args
                        for arg in args_mut(instr) {
                            // Argument may not have a value number if it was defined in another basic block

                            if let Some(&arg_value_number) = num_from_var.get(arg) {
                                *arg = canonical_var_from_num[arg_value_number].clone();
                            }
                        }
                    }

                    num_from_var.insert(dest.clone(), num);
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
