use bril_rs::{Code, Function, Instruction, Literal, ValueOps};
use bril_utils::{instructions_to_blocks, BasicBlock};
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

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

fn lvn_block_pass(block: &mut BasicBlock) {
    // Mapping from value tuples to canonical variables, with each row numbered
    let mut value_to_var: HashMap<Value, (String, usize)> = HashMap::new();
    // mapping from variable names to their current value number
    let mut var_to_num: HashMap<String, usize> = HashMap::new();

    let mut num_to_canonical_var = vec![];

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
                None => continue,
                Some((dest, value)) => {
                    let num: usize;

                    // Already in table
                    if value_to_var.contains_key(&value.clone()) {
                        // This value have been computed before. Reuse it
                        let (var, num2) = value_to_var.get(&value.clone()).unwrap();
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
                        // A newly computed value.
                        num = num_to_canonical_var.len();
                        num_to_canonical_var.push(dest.clone());

                        // Add to table
                        value_to_var.insert(value.clone(), (dest.clone(), num));

                        // Use (canonical) variables in the table rather than old args
                        for arg in args_mut(instr) {
                            *arg = num_to_canonical_var[*var_to_num.get(arg).unwrap()].clone();
                        }
                    }

                    var_to_num.insert(dest.clone(), num);
                }
            }
        }
    }
}

pub fn local_value_numbering(func: &mut Function) {
    let mut blocks = instructions_to_blocks(&func.instrs);

    for block in &mut blocks {
        lvn_block_pass(block);

        // TODO: temporary hack
        func.instrs = block.clone();
    }
}

fn main() {
    let mut program = bril_rs::load_program();

    for function in &mut program.functions {
        local_value_numbering(function);
    }

    bril_rs::output_program(&program);
}
