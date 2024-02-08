// // Local value numbering
//
// use bril_rs::{Code, Function};
// use std::collections::HashMap;
//
// use crate::cfg::{instructions_to_blocks, BasicBlock};
//
// fn lvn_block_pass(block: &mut BasicBlock) {
//     // Mapping from value tuples to canonical variables, with each row numbered
//     //let mut lvn_table = HashMap::new();
//     // mapping from variable names to their current value numver
//     //let mut var_to_num = HashMap::new();
//
//     for code in block {
//         if let Code::Instruction(instr) = code {}
//     }
// }
//
// fn local_value_numbering(func: &mut Function) {
//     let mut blocks = instructions_to_blocks(&func.instrs);
//
//     for block in &mut blocks {
//         lvn_block_pass(block)
//     }
// }
