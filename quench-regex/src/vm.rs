//! The matcher: a Pike VM, which follows every possible path through the program in lock step over the
//! text, so the time is proportional to text length times program size and never explodes.

use crate::class::is_word;
use crate::compile::{Inst, Program};
use crate::parse::Assertion;

/// "No position recorded" in a capture slot.
pub const NONE: usize = usize::MAX;

/// The threads alive at one position, in priority order, with their capture slots.
struct Threads {
    /// `marks[pc] == generation` means the program counter was already reached at this position (so a
    /// lower-priority path to it is dropped). Bumping the generation clears every mark at once.
    marks: Vec<u32>,
    generation: u32,
    pcs: Vec<usize>,
    /// `stride` slots per thread, parallel to `pcs`.
    slots: Vec<usize>,
}

impl Threads {
    fn new(program_len: usize) -> Self {
        Self {
            marks: vec![0; program_len],
            generation: 1,
            pcs: Vec::new(),
            slots: Vec::new(),
        }
    }

    fn clear(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.marks.fill(0);
            self.generation = 1;
        }
        self.pcs.clear();
        self.slots.clear();
    }

    /// Marks `pc` reached; `false` when it already was.
    fn visit(&mut self, pc: usize) -> bool {
        if self.marks[pc] == self.generation {
            return false;
        }
        self.marks[pc] = self.generation;
        true
    }

    const fn is_empty(&self) -> bool {
        self.pcs.is_empty()
    }
}

enum Frame {
    Explore(usize),
    Restore(usize, usize),
}

fn holds(assertion: Assertion, text: &str, pos: usize) -> bool {
    let before = text[..pos].chars().next_back();
    let after = text[pos..].chars().next();
    match assertion {
        Assertion::StartText => pos == 0,
        Assertion::EndText => pos == text.len(),
        Assertion::StartLine => pos == 0 || before == Some('\n'),
        Assertion::EndLine => pos == text.len() || after == Some('\n'),
        Assertion::WordBoundary | Assertion::NotWordBoundary => {
            let a = before.is_some_and(is_word);
            let b = after.is_some_and(is_word);
            (a != b) == (assertion == Assertion::WordBoundary)
        }
    }
}

/// Adds the thread at `pc` and everything reachable from it without consuming a character.
fn add(
    program: &Program,
    list: &mut Threads,
    stack: &mut Vec<Frame>,
    start_pc: usize,
    text: &str,
    pos: usize,
    slots: &mut [usize],
) {
    stack.push(Frame::Explore(start_pc));
    while let Some(frame) = stack.pop() {
        match frame {
            Frame::Restore(slot, old) => slots[slot] = old,
            Frame::Explore(mut pc) => loop {
                if !list.visit(pc) {
                    break;
                }
                match &program.insts[pc] {
                    Inst::Jmp(target) => pc = *target,
                    Inst::Split(first, second) => {
                        stack.push(Frame::Explore(*second));
                        pc = *first;
                    }
                    Inst::Save(slot) => {
                        if *slot < slots.len() {
                            stack.push(Frame::Restore(*slot, slots[*slot]));
                            slots[*slot] = pos;
                        }
                        pc += 1;
                    }
                    Inst::Assert(assertion) => {
                        if holds(*assertion, text, pos) {
                            pc += 1;
                        } else {
                            break;
                        }
                    }
                    Inst::Char(_) | Inst::Class(_) | Inst::Any(_) | Inst::Match => {
                        list.pcs.push(pc);
                        list.slots.extend_from_slice(slots);
                        break;
                    }
                }
            },
        }
    }
}

/// Finds the leftmost match starting at or after `from`. Returns the capture slots (`NONE` for groups that
/// did not take part); with `earliest`, returns as soon as any match is known (for `is_match`) and only the
/// whole-match slots are meaningful.
pub fn search(program: &Program, text: &str, from: usize, earliest: bool) -> Option<Vec<usize>> {
    if program.anchored_start && from > 0 {
        return None;
    }
    let stride = program.slots;
    let mut current = Threads::new(program.insts.len());
    let mut next = Threads::new(program.insts.len());
    let mut stack: Vec<Frame> = Vec::new();
    let mut working = vec![NONE; stride];
    let mut matched: Option<Vec<usize>> = None;
    let mut pos = from;

    loop {
        if matched.is_none() && (!program.anchored_start || pos == 0) {
            if current.is_empty()
                && let Some(prefix) = &program.prefix
            {
                match text[pos..].find(prefix.as_str()) {
                    Some(offset) => pos += offset,
                    None => break,
                }
            }
            working.fill(NONE);
            add(
                program,
                &mut current,
                &mut stack,
                0,
                text,
                pos,
                &mut working,
            );
        }
        if current.is_empty() && (matched.is_some() || program.anchored_start) {
            break;
        }

        let here = text[pos..].chars().next();
        let width = here.map_or(0, char::len_utf8);
        for index in 0..current.pcs.len() {
            let pc = current.pcs[index];
            let base = index * stride;
            let advance = match &program.insts[pc] {
                Inst::Char(c) => here == Some(*c),
                Inst::Class(class) => here.is_some_and(|c| program.classes[*class].contains(c)),
                Inst::Any(newline) => here.is_some_and(|c| *newline || c != '\n'),
                Inst::Match => {
                    matched = Some(current.slots[base..base + stride].to_vec());
                    if earliest {
                        return matched;
                    }
                    // Lower-priority threads can no longer win.
                    break;
                }
                _ => false,
            };
            if advance {
                working.copy_from_slice(&current.slots[base..base + stride]);
                add(
                    program,
                    &mut next,
                    &mut stack,
                    pc + 1,
                    text,
                    pos + width,
                    &mut working,
                );
            }
        }
        std::mem::swap(&mut current, &mut next);
        next.clear();
        if here.is_none() {
            break;
        }
        pos += width;
    }
    matched
}
