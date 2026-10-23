//! Syntax tree to a program for the matcher.

use crate::class::ClassSet;
use crate::error::Error;
use crate::parse::{Assertion, Node, Parsed};

/// Most instructions a program may have.
const MAX_INSTRUCTIONS: usize = 200_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inst {
    Char(char),
    Class(usize),
    /// `.`; `true` when it also matches a newline.
    Any(bool),
    /// Try the first target, then the second.
    Split(usize, usize),
    Jmp(usize),
    /// Record the current position in a capture slot.
    Save(usize),
    Assert(Assertion),
    Match,
}

#[derive(Debug)]
pub struct Program {
    pub insts: Vec<Inst>,
    pub classes: Vec<ClassSet>,
    /// Slots: two per group, group 0 being the whole match.
    pub slots: usize,
    pub names: Vec<(String, usize)>,
    /// Text every match must start with, when the pattern begins with literal characters.
    pub prefix: Option<String>,
    /// The pattern can only match at the very start of the text.
    pub anchored_start: bool,
}

struct Compiler {
    insts: Vec<Inst>,
    classes: Vec<ClassSet>,
}

pub fn compile(parsed: Parsed) -> Result<Program, Error> {
    let mut c = Compiler {
        insts: Vec::new(),
        classes: Vec::new(),
    };
    c.insts.push(Inst::Save(0));
    c.node(&parsed.node)?;
    c.insts.push(Inst::Save(1));
    c.insts.push(Inst::Match);
    Ok(Program {
        insts: c.insts,
        classes: c.classes,
        slots: 2 * (parsed.groups + 1),
        names: parsed.names,
        prefix: prefix_of(&parsed.node),
        anchored_start: starts_anchored(&parsed.node),
    })
}

impl Compiler {
    fn emit(&mut self, inst: Inst) -> Result<usize, Error> {
        if self.insts.len() >= MAX_INSTRUCTIONS {
            return Err(Error::too_big());
        }
        self.insts.push(inst);
        Ok(self.insts.len() - 1)
    }

    fn node(&mut self, node: &Node) -> Result<(), Error> {
        match node {
            Node::Empty => {}
            Node::Char(c) => {
                self.emit(Inst::Char(*c))?;
            }
            Node::Class(set) => {
                if let Some(c) = set.as_single() {
                    self.emit(Inst::Char(c))?;
                } else {
                    self.classes.push(set.clone());
                    let index = self.classes.len() - 1;
                    self.emit(Inst::Class(index))?;
                }
            }
            Node::Any(newline) => {
                self.emit(Inst::Any(*newline))?;
            }
            Node::Assert(kind) => {
                self.emit(Inst::Assert(*kind))?;
            }
            Node::Concat(items) => {
                for item in items {
                    self.node(item)?;
                }
            }
            Node::Alternate(branches) => self.alternate(branches)?,
            Node::Group(inner, index) => {
                self.emit(Inst::Save(2 * index))?;
                self.node(inner)?;
                self.emit(Inst::Save(2 * index + 1))?;
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => self.repeat(node, *min, *max, *greedy)?,
        }
        Ok(())
    }

    fn alternate(&mut self, branches: &[Node]) -> Result<(), Error> {
        let mut jumps = Vec::new();
        for (index, branch) in branches.iter().enumerate() {
            if index + 1 < branches.len() {
                let split = self.emit(Inst::Split(0, 0))?;
                self.node(branch)?;
                jumps.push(self.emit(Inst::Jmp(0))?);
                let next = self.insts.len();
                self.insts[split] = Inst::Split(split + 1, next);
            } else {
                self.node(branch)?;
            }
        }
        let end = self.insts.len();
        for jump in jumps {
            self.insts[jump] = Inst::Jmp(end);
        }
        Ok(())
    }

    fn repeat(
        &mut self,
        node: &Node,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    ) -> Result<(), Error> {
        match max {
            None if min == 0 => {
                if nullable(node) {
                    // A body that can match nothing: `(x+)?`, so the empty pass is still recorded (and
                    // priorities come out right), exactly as RE2 and the `regex` crate compile it.
                    let skip = self.emit(Inst::Split(0, 0))?;
                    let body = self.insts.len();
                    self.node(node)?;
                    let again = self.emit(Inst::Split(0, 0))?;
                    let exit = self.insts.len();
                    self.insts[again] = split_inst(body, exit, greedy);
                    self.insts[skip] = split_inst(skip + 1, exit, greedy);
                } else {
                    let split = self.emit(Inst::Split(0, 0))?;
                    self.node(node)?;
                    self.emit(Inst::Jmp(split))?;
                    let exit = self.insts.len();
                    self.insts[split] = split_inst(split + 1, exit, greedy);
                }
            }
            None => {
                // x{n,} = x repeated n-1 times, then x+.
                for _ in 1..min {
                    self.node(node)?;
                }
                let body = self.insts.len();
                self.node(node)?;
                let again = self.emit(Inst::Split(0, 0))?;
                let exit = self.insts.len();
                self.insts[again] = split_inst(body, exit, greedy);
            }
            Some(max) => {
                for _ in 0..min {
                    self.node(node)?;
                }
                // x{0,k}: nested optionals sharing one exit.
                let mut splits = Vec::new();
                for _ in min..max {
                    splits.push(self.emit(Inst::Split(0, 0))?);
                    self.node(node)?;
                }
                let exit = self.insts.len();
                for split in splits {
                    self.insts[split] = split_inst(split + 1, exit, greedy);
                }
            }
        }
        Ok(())
    }
}

/// A split that prefers `body` when greedy and `exit` when lazy.
const fn split_inst(body: usize, exit: usize, greedy: bool) -> Inst {
    if greedy {
        Inst::Split(body, exit)
    } else {
        Inst::Split(exit, body)
    }
}

/// Whether the node can match the empty string.
fn nullable(node: &Node) -> bool {
    match node {
        Node::Empty | Node::Assert(_) => true,
        Node::Char(_) | Node::Class(_) | Node::Any(_) => false,
        Node::Concat(items) => items.iter().all(nullable),
        Node::Alternate(branches) => branches.iter().any(nullable),
        Node::Repeat { node, min, .. } => *min == 0 || nullable(node),
        Node::Group(inner, _) => nullable(inner),
    }
}

/// The literal text every match begins with, if the pattern starts with plain characters.
fn prefix_of(node: &Node) -> Option<String> {
    let items: &[Node] = match node {
        Node::Concat(items) => items,
        Node::Char(_) => std::slice::from_ref(node),
        _ => return None,
    };
    let text: String = items
        .iter()
        .map_while(|n| {
            if let Node::Char(c) = n {
                Some(*c)
            } else {
                None
            }
        })
        .collect();
    (!text.is_empty()).then_some(text)
}

fn starts_anchored(node: &Node) -> bool {
    match node {
        Node::Assert(Assertion::StartText) => true,
        Node::Concat(items) => items.first().is_some_and(starts_anchored),
        Node::Group(inner, _) => starts_anchored(inner),
        Node::Alternate(branches) => branches.iter().all(starts_anchored),
        _ => false,
    }
}
