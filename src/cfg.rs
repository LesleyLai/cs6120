use bril_rs::{Code, EffectOps, Instruction};
use std::collections::HashMap;

pub type BasicBlock<'a> = Vec<Code>;

fn is_terminator(instr: &Instruction) -> bool {
    matches!(
        instr,
        Instruction::Effect {
            op: EffectOps::Jump | EffectOps::Branch | EffectOps::Return,
            ..
        }
    )
}

// form basic blocks from flat instructions
pub fn instructions_to_blocks(instructions: &[Code]) -> Vec<BasicBlock> {
    let mut blocks: Vec<BasicBlock> = vec![];
    let mut current_block = vec![];

    for code in instructions {
        match code {
            Code::Instruction(instr) => {
                // Add the instruction to the new block
                current_block.push(code.clone());

                // If it is a terminator, finish this block and start a new one
                if is_terminator(instr) {
                    blocks.push(current_block);
                    current_block = vec![];
                }
            }
            Code::Label { .. } => {
                // End the current block
                if !current_block.is_empty() {
                    blocks.push(current_block);
                }

                // start a new block with this label
                current_block = vec![code.clone()];
            }
        }
    }
    if !current_block.is_empty() {
        blocks.push(current_block);
    }

    blocks
}

// Given a sequence of basic blocks, forms a map that mapping names to blocks
pub fn map_blocks_by_name(blocks: Vec<BasicBlock>) -> HashMap<String, BasicBlock> {
    let mut block_by_name = HashMap::new();

    let mut i = 1;

    for block in blocks {
        match &block[0] {
            Code::Label { label, .. } => {
                // Remove the label but use it for block's name
                block_by_name.insert(label.clone(), block[1..].to_vec());
            }
            _ => {
                // Make up a fresh name
                block_by_name.insert(format!("b{i}"), block);
                i += 1;
            }
        }
    }

    block_by_name
}

// Add explicit terminator to all blocks
