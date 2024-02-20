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
    Op(ValueOps, Vec<usize>, Type),
}

impl ValueExpr {
    fn from_literal(literal: Literal) -> Self {
        let typ = literal.get_type();
        ValueExpr::Constant(literal, typ)
    }

    fn get_type(&self) -> Type {
        match self {
            ValueExpr::Constant(_, typ) => typ.clone(),
            ValueExpr::Op(_, _, typ) => typ.clone(),
        }
    }

    fn get_constant_bool(&self) -> bool {
        match self {
            ValueExpr::Constant(Literal::Bool(b), _) => *b,
            _ => panic!("The Value Expr does not have a bool constant"),
        }
    }
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

    fn get_dest_and_value(self: &mut Self, instr: Instruction) -> Option<(String, ValueExpr)> {
        match instr {
            Instruction::Constant { dest, value, .. } => {
                let value = ValueExpr::from_literal(value);
                Some((dest, value))
            }
            Instruction::Value {
                op,
                args,
                dest,
                op_type,
                ..
            } => {
                let number_args: Vec<usize> = args
                    .iter()
                    .map(|arg: &String| match self.num_from_var.get(arg) {
                        None => {
                            // argument is not defined in this block. Create a dummy number for it

                            let num = self.canonical_var_from_num.len();
                            self.num_from_var.insert(arg.clone(), num);
                            self.canonical_var_from_num.push(arg.clone());
                            num
                        }
                        Some(num) => *num,
                    })
                    .collect();

                let value = ValueExpr::Op(op, number_args, op_type);
                Some((dest, value))
            }
            Instruction::Effect { .. } => None,
        }
    }
}

fn make_constant_instruction(dest: String, value: Literal) -> Code {
    Code::Instruction(Instruction::Constant {
        dest: dest.clone(),
        op: ConstOps::Const,
        pos: None,
        const_type: value.get_type(),
        value: value.clone(),
    })
}

// Returns an iterator of arguments if all arguments has constant value. None otherwise.
fn all_constant_args<'a, 'b>(lvn: &'a LVN, args: &'b [usize]) -> Option<Vec<&'a Literal>> {
    let args_const_value = args.iter().map(|arg| match lvn.value_from_num.get(arg) {
        Some(ValueExpr::Constant(lit, _)) => Some(lit),
        _ => None,
    });

    if args_const_value.clone().all(|opt| opt.is_some()) {
        let arg_values: Vec<&Literal> = args_const_value.filter_map(|opt| opt).collect();
        Some(arg_values)
    } else {
        None
    }
}

fn lvn_block_pass(block: &mut BasicBlock, option: &Options) {
    let mut lvn = LVN::new();

    for code in block {
        if let Code::Instruction(instr) = code {
            match lvn.get_dest_and_value(instr.clone()) {
                None => {
                    // Effects
                    lvn.replace_args_with_canonical_variables(instr);
                }
                Some((dest, mut value)) => {
                    if option.handle_commutativity {
                        match value {
                            ValueExpr::Op(
                                ValueOps::Add | ValueOps::Mul | ValueOps::Eq,
                                ref mut args,
                                _,
                            ) => {
                                args.sort();
                            }
                            _ => {}
                        }
                    }

                    if option.handle_const_folding {
                        match value {
                            ValueExpr::Op(ValueOps::And, ref args, _) => {
                                let (arg1_num, arg2_num) = (args[0], args[1]);

                                let bool1 = lvn
                                    .value_from_num
                                    .get(&arg1_num)
                                    .map(|value| value.get_constant_bool());

                                let bool2 = lvn
                                    .value_from_num
                                    .get(&arg2_num)
                                    .map(|value| value.get_constant_bool());

                                let new_bool = match (bool1, bool2) {
                                    (Some(false), _) | (_, Some(false)) => Some(false),
                                    (Some(true), Some(true)) => Some(true),
                                    _ => None,
                                };

                                if let Some(new_bool) = new_bool {
                                    let new_value = Literal::Bool(new_bool);
                                    *code =
                                        make_constant_instruction(dest.clone(), new_value.clone());

                                    lvn.add_fresh_num_to_table(
                                        dest,
                                        ValueExpr::from_literal(new_value),
                                    );

                                    continue;
                                }
                            }
                            ValueExpr::Op(ValueOps::Or, ref args, _) => {
                                let (arg1_num, arg2_num) = (args[0], args[1]);

                                let bool1 = lvn
                                    .value_from_num
                                    .get(&arg1_num)
                                    .map(|value| value.get_constant_bool());

                                let bool2 = lvn
                                    .value_from_num
                                    .get(&arg2_num)
                                    .map(|value| value.get_constant_bool());

                                let new_bool = match (bool1, bool2) {
                                    (Some(true), _) | (_, Some(true)) => Some(true),
                                    (Some(false), Some(false)) => Some(false),
                                    _ => None,
                                };

                                if let Some(new_bool) = new_bool {
                                    let new_value = Literal::Bool(new_bool);
                                    *code =
                                        make_constant_instruction(dest.clone(), new_value.clone());

                                    lvn.add_fresh_num_to_table(
                                        dest,
                                        ValueExpr::from_literal(new_value),
                                    );

                                    continue;
                                }
                            }
                            ValueExpr::Op(ValueOps::Not, ref args, _) => {
                                let arg1_num = args[0];

                                let bool1 = lvn
                                    .value_from_num
                                    .get(&arg1_num)
                                    .map(|value| value.get_constant_bool());

                                let new_bool = bool1.map(|b| !b);

                                if let Some(new_bool) = new_bool {
                                    let new_value = Literal::Bool(new_bool);
                                    *code =
                                        make_constant_instruction(dest.clone(), new_value.clone());

                                    lvn.add_fresh_num_to_table(
                                        dest,
                                        ValueExpr::from_literal(new_value),
                                    );

                                    continue;
                                }
                            }
                            _ => { /* Can't fold. Do nothing */ }
                        }
                    }

                    let num: usize;
                    match lvn.var_and_num_from_value.get(&value.clone()) {
                        // Already in table
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
                                op_type: value.get_type(),
                            });

                            lvn.num_from_var.insert(dest.clone(), num);
                        }
                        // A brand-new value
                        None => {
                            if option.handle_copy_propagate {
                                match value {
                                    ValueExpr::Op(ValueOps::Id, args, typ) => {
                                        let num = args[0];

                                        // check whether the original is a constant
                                        match &lvn.value_from_num.get(&num) {
                                            Some(ValueExpr::Constant(literal, _)) => {
                                                *code = make_constant_instruction(
                                                    dest.clone(),
                                                    literal.clone(),
                                                );
                                            }
                                            _ => {
                                                *code = Code::Instruction(Instruction::Value {
                                                    args: vec![
                                                        lvn.canonical_var_from_num[num].clone()
                                                    ],
                                                    dest: dest.clone(),
                                                    funcs: vec![],
                                                    labels: vec![],
                                                    op: ValueOps::Id,
                                                    pos: None,
                                                    op_type: typ.clone(),
                                                })
                                            }
                                        };

                                        lvn.num_from_var.insert(dest.clone(), num);

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
