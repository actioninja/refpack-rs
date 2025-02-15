use std::array;
use std::cmp::min;
use std::ops::Range;

use crate::data::compression::bytes_for_match;
use crate::data::compression::prefix_search::PrefixSearcher;
use crate::data::control::Command::Stop;
use crate::data::control::{Command, Control, COPY_LITERAL_MAX, LITERAL_MAX};

// state is packed into 32 bits for SIMD optimization purposes
// 31: literal/copy command flag
// when 0:
//   13-30: offset
//   11-12: literals
//   0-10: copy length
// when 1:
//   0-7 literals
#[derive(Copy, Clone, Default)]
struct CommandState(u32);

impl CommandState {
    fn literal(literal: u8) -> Self {
        Self((1 << 31) | literal as u32)
    }

    fn command(offset: u32, literal: u8, length: u16) -> Self {
        // it is assumed that none of the values ever exceed the maximum specified by refpack
        // doing these checks is possible but expensive since this function is in a hot part of the algorithm

        Self((offset << 13) | ((literal as u32) << 11) | (length as u32))
    }

    fn is_literal(self) -> bool {
        (self.0 & (1 << 31)) != 0
    }

    fn num_literals(self) -> u8 {
        if self.is_literal() {
            self.0 as u8
        } else {
            0
        }
    }

    fn to_command(self) -> Command {
        if self.is_literal() {
            Command::new_literal((self.0 & 0xFF) as usize)
        } else {
            Command::new(
                ((self.0 >> 13) & ((1 << 18) - 1)) as usize,
                (self.0 & ((1 << 11) - 1)) as usize,
                ((self.0 >> 11) & 3) as usize,
            )
        }
    }
}

fn controls_from_state_slice(state: &[u32], input: &[u8]) -> Vec<Control> {
    let mut cur_pos = state.len() - 1;
    let mut controls = vec![];

    let num_stop_literals = CommandState(state[cur_pos]).num_literals() % 4;

    let literal_pos = cur_pos + 1 - num_stop_literals as usize;
    controls.push(Control {
        command: Stop(num_stop_literals),
        bytes: input[literal_pos..literal_pos + num_stop_literals as usize].to_vec(),
    });

    cur_pos -= num_stop_literals as usize;

    loop {
        let cur_command = CommandState(state[cur_pos]).to_command();

        if let Command::Literal(literal) = cur_command {
            assert_eq!(literal % 4, 0);
        }

        let num_literal = cur_command.num_of_literal().unwrap_or(0);
        let num_copy = cur_command.offset_copy().unwrap_or((0, 0)).1;

        let command_decompressed_bytes = num_literal + num_copy;

        let literal_pos = cur_pos + 1 - command_decompressed_bytes;
        controls.push(Control {
            command: cur_command,
            bytes: input[literal_pos..literal_pos + num_literal].to_vec(),
        });

        if command_decompressed_bytes > cur_pos {
            debug_assert!(command_decompressed_bytes == cur_pos + 1);
            break;
        }
        cur_pos -= command_decompressed_bytes;
    }

    controls.reverse();

    controls
}

fn update_state_simd(
    cost_state: &mut [u32],
    command_state: &mut [u32],
    cur_cost: u32,
    command_bytes: u32,
    range: Range<usize>,
    command_base: CommandState,
) {
    const CHUNK_SIZE: usize = 4;

    let new_cost = cur_cost + command_bytes;
    
    let mut cost_state_chunks = cost_state[range.clone()].chunks_exact_mut(CHUNK_SIZE);
    let mut command_state_chunks = command_state[range].chunks_exact_mut(CHUNK_SIZE);

    let chunk_bytes = cost_state_chunks.len() * CHUNK_SIZE;

    let new_commands_base = command_base.0;
    let new_commands_base_arr: [u32; CHUNK_SIZE] = array::from_fn(|i| new_commands_base + i as u32);

    for (n, (cost, command)) in (&mut cost_state_chunks)
        .zip(&mut command_state_chunks)
        .enumerate()
    {
        let cur_cost_arr: [u32; CHUNK_SIZE] = cost.try_into().unwrap();
        let new_cost_arr = cur_cost_arr.map(|c| c.min(new_cost));

        let none_changed = (0..CHUNK_SIZE).all(|i| cur_cost_arr[i] == new_cost_arr[i]);

        if none_changed {
            continue;
        }

        let new_commands_arr: [u32; CHUNK_SIZE] = array::from_fn(|i| {
            let new_command = new_commands_base_arr[i] + (n * CHUNK_SIZE) as u32;
            let old_command = command[i];

            if cur_cost_arr[i] > new_cost {
                new_command
            } else {
                old_command
            }
        });
        command.copy_from_slice(&new_commands_arr);

        cost.copy_from_slice(&new_cost_arr);
    }

    let new_commands_remainder = new_commands_base_arr.map(|c| c + chunk_bytes as u32);

    for ((cost, command), new_command) in cost_state_chunks
        .into_remainder()
        .iter_mut()
        .zip(command_state_chunks.into_remainder().iter_mut())
        .zip(new_commands_remainder)
    {
        if *cost > new_cost {
            *cost = new_cost;
            *command = new_command;
        }
    }
}

pub(crate) fn encode_slice_hc(input: &[u8]) -> Vec<Control> {
    let input_length = input.len();

    if input_length <= 3 {
        return vec![Control {
            command: Stop(input_length as u8),
            bytes: Vec::from(input),
        }];
    }

    let mut prev = PrefixSearcher::<4>::build(input);

    let mut command_state = vec![CommandState::default().0; input_length];
    let mut cost_state = vec![u32::MAX; input_length];

    cost_state[0] = 1;
    command_state[0] = CommandState::literal(1).0;

    for pos in 0..(input_length as u32 - 1) {
        let cur_cost = cost_state[pos as usize];
        let cur_command = command_state[pos as usize];
        let cur_literals = CommandState(cur_command).num_literals();

        // there can't be any matches on the last 3 bytes
        if pos < (input_length - 3) as u32 {
            let match_start_pos = pos + 1;

            prev.search(
                match_start_pos as usize,
                |match_pos, match_start, match_end| {
                    let offset = match_start_pos as usize - match_pos;

                    let mut i = match_start;
                    while i < match_end {
                        if let Some((command_bytes, interval_limit)) = bytes_for_match(i, offset) {
                            if let Some(command_bytes) = command_bytes {
                                let match_length_start = i;
                                let match_length_end = min(interval_limit + 1, match_end);
                                let range = (pos as usize + match_length_start)
                                    ..(pos as usize + match_length_end);
                                update_state_simd(
                                    &mut cost_state,
                                    &mut command_state,
                                    cur_cost,
                                    command_bytes as u32,
                                    // pos,
                                    // match_length_start,
                                    // match_length_end,
                                    // offset as u32,
                                    // cur_literals,
                                    range,
                                    CommandState::command(
                                        offset as u32,
                                        cur_literals % 4,
                                        match_length_start as u16,
                                    ),
                                );
                            }

                            i = interval_limit + 1;
                            continue;
                        }
                        break;
                    }
                },
            );
        }

        // update for the next literal
        let mut literal_cost = cur_cost;
        literal_cost += 1;
        let mut new_state_literal = cur_literals + 1;
        if new_state_literal > (LITERAL_MAX + COPY_LITERAL_MAX) {
            // literal + copy command cannot represent this amount, wrap back around to a new literal command
            new_state_literal = 4;
        }
        if new_state_literal == 4 {
            // we needed to add a new byte for the literal command
            literal_cost += 1;
        }

        if cost_state[pos as usize + 1] > literal_cost {
            cost_state[pos as usize + 1] = literal_cost;
            command_state[pos as usize + 1] = CommandState::literal(new_state_literal).0;
        }
    }

    controls_from_state_slice(&command_state, input)
}
