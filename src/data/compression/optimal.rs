////////////////////////////////////////////////////////////////////////////////
// This Source Code Form is subject to the terms of the Mozilla Public         /
// License, v. 2.0. If a copy of the MPL was not distributed with this         /
// file, You can obtain one at https://mozilla.org/MPL/2.0/.                   /
//                                                                             /
////////////////////////////////////////////////////////////////////////////////

use std::array;
use std::cmp::min;
use std::ops::Range;

use crate::data::compression::bytes_for_match;
use crate::data::compression::prefix_search::PrefixSearcher;
use crate::data::control::Command::Stop;
use crate::data::control::{Command, Control, COPY_LITERAL_MAX, LITERAL_MAX};

pub(crate) const HASH_CHAINING_LEVELS: usize = 4;

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
    // add the output controls in reverse order in this list
    let mut controls = vec![];

    // special handling of the last literals: the last command must be a stop command
    // so we can take the number of literals at the end of the input and put them into the stop command
    let num_stop_literals = CommandState(state[cur_pos]).num_literals() % 4;

    // the current position includes the last byte of this literal, so subtract one
    let literal_pos = cur_pos + 1 - num_stop_literals as usize;
    controls.push(Control {
        command: Stop(num_stop_literals),
        bytes: input[literal_pos..literal_pos + num_stop_literals as usize].to_vec(),
    });

    cur_pos -= num_stop_literals as usize;

    loop {
        // the bytes of the next command end at the current position
        let cur_command = CommandState(state[cur_pos]).to_command();

        if let Command::Literal(literal) = cur_command {
            assert_eq!(literal % 4, 0);
        }

        let num_literal = cur_command.num_of_literal().unwrap_or(0);
        let num_copy = cur_command.offset_copy().unwrap_or((0, 0)).1;

        // total number of bytes in the input that the current command encodes
        let command_decompressed_bytes = num_literal + num_copy;

        // same as with the stop command
        let literal_pos = cur_pos + 1 - command_decompressed_bytes;
        controls.push(Control {
            command: cur_command,
            bytes: input[literal_pos..literal_pos + num_literal].to_vec(),
        });

        if command_decompressed_bytes > cur_pos {
            // the encoding should end at position -1, but unsigned integers cannot represent this
            debug_assert!(command_decompressed_bytes == cur_pos + 1);
            break;
        }
        cur_pos -= command_decompressed_bytes;
    }

    // we built the controls in reverse order, so reverse the vec
    controls.reverse();

    controls
}

fn update_state_simd(
    cost_state: &mut [u32],
    command_state: &mut [u32],
    new_cost: u32,
    range: Range<usize>,
    command_base: CommandState,
) {
    const CHUNK_SIZE: usize = 4;

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

/// Search for the set of controls that can encode the input slice in the lowest amount of output bytes possible.
///
/// To understand this algorithm, first consider the case of using a hash chain prefix searcher
/// as it is equivalent to the case of using a multilevel hash chain.
///
/// This algorithm is in essence a variation of dijkstra's algorithm; for every node (byte position) that is opened
/// it is always known what the most cost-effective way to reach that point is.
///
/// For example, consider the case of an input consisting of all zeros:
/// it is known that to encode the first byte of the input the only option is a literal command of length 1.
/// Thus, the cost for encoding the first byte must be 2 (literal command + 1 literal).
/// Because copy commands can also include four literals the literal command cost
/// is not encoded until after four literals.
/// After the first byte has been encoded multiple other bytes can be reached via a copy command;
/// the short copy command can copy 3-10 bytes with a minimum offset of 1 and a cost of 2 bytes,
/// thus we know that positions 3-10 can be reached with a maximum cost of 3 (1 byte literal + 2 bytes short command),
/// and positions 1 and 2 can also be reached with a cost of 3 and 4 respectively (with 2 and 3 literal bytes).
///
/// Once all positions have been opened it is known that the last cost state is the minimum cost
/// for encoding all bytes in the input. It is then possible to encode all commands by tracing backwards
/// through the input while referencing the command state that is built in the search process.
pub(crate) fn encode_slice_hc<'a, PS: PrefixSearcher<'a>>(input: &'a [u8]) -> Vec<Control> {
    let input_length = input.len();

    // if the input is 3 bytes or fewer it is impossible to encode any copy commands
    // just return the stop commands with the input as literal bytes
    if input_length <= 3 {
        return vec![Control {
            command: Stop(input_length as u8),
            bytes: Vec::from(input),
        }];
    }

    // build the prefix searcher
    // it will give us all previous occurrences of the current position along with their match length
    let mut prev = PS::build(input);

    // tracks the last command to encode all bytes in the input up to a certain point
    let mut command_state = vec![CommandState::default().0; input_length];
    // tracks the maximum cost to encode all bytes in the input up to a certain position
    let mut cost_state = vec![u32::MAX; input_length];
    // the state vecs could be combined into a single vec, but we store them separately for SIMD purposes

    // we know the first byte must be encoded as a literal, thus the cost is 1
    cost_state[0] = 1;
    // idem
    command_state[0] = CommandState::literal(1).0;

    // go through all the byte positions in the input
    for pos in 0..(input_length as u32 - 1) {
        // since this position has no unexplored predecessors
        // we know the cost to reach this byte is equivalent to the stored cost state
        let cur_cost = cost_state[pos as usize];
        // and the command to reach that state is the stored command
        let cur_command = command_state[pos as usize];
        // get the number of literals that are passed on into the next command
        // for copy commands this is always 0
        let cur_literals = CommandState(cur_command).num_literals();

        // there can't be any matches on the last 3 bytes since matches must always be at least 3 bytes
        if pos < (input_length - 3) as u32 {
            // we want to try encoding bytes for the next byte onward since the current position is already known
            // so the search position is the current position + 1
            let match_start_pos = pos + 1;

            // search for all matches with the search position
            prev.search(
                match_start_pos as usize,
                |match_pos, match_start, match_end| {
                    // for all bytes in this match, update the command and cost state
                    // for all positions that have a lower cost than the stored cost state

                    debug_assert!(match_start < match_end);

                    let offset = match_start_pos as usize - match_pos;

                    // loop through all ranges in the match that have an equal cost
                    // for SIMD optimization purposes
                    let mut i = match_start;
                    while i < match_end {
                        if let Some((command_bytes, interval_limit)) = bytes_for_match(i, offset) {
                            // get the cost to encode the current command (command_bytes)
                            if let Some(command_bytes) = command_bytes {
                                let match_length_start = i;
                                let match_length_end = min(interval_limit + 1, match_end);
                                let range = (pos as usize + match_length_start)
                                    ..(pos as usize + match_length_end);

                                // the cost for all encoded commands in this range
                                let new_cost = cur_cost + command_bytes as u32;

                                // now update the cost and command state in this range with the new cost
                                update_state_simd(
                                    &mut cost_state,
                                    &mut command_state,
                                    new_cost,
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
            // a copy command cannot represent this, so we needed to add a new byte for the literal command
            literal_cost += 1;
        }

        if cost_state[pos as usize + 1] > literal_cost {
            cost_state[pos as usize + 1] = literal_cost;
            command_state[pos as usize + 1] = CommandState::literal(new_state_literal).0;
        }
    }

    // since we don't need the cost state for building the output command list
    // we can drop it early to save on peak memory usage
    drop(cost_state);

    // trace backwards through the command state to extract the output command list
    controls_from_state_slice(&command_state, input)
}
