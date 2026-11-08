//! The symbol tables: each command's Unicode character and TeX class, the large and named operators with their
//! limits behaviour, and the delimiters `\left`, `\right` and `\big` accept.
//!
//! The classes are those of plain TeX and amsmath (`\mid` a relation, `|` ordinary, `\ldots` inner, `!` closing),
//! so the spacing the outputs derive from them is TeX's.

use crate::ast::{Class, Limits, MathChar};

use Class::{Bin, Close, Inner, Open, Ord, Punct, Rel};

/// Commands that stand for one character.
const SYMBOLS: &[(&str, char, Class)] = &[
    // Greek, lower case.
    ("alpha", 'α', Ord),
    ("beta", 'β', Ord),
    ("gamma", 'γ', Ord),
    ("delta", 'δ', Ord),
    ("epsilon", 'ϵ', Ord),
    ("varepsilon", 'ε', Ord),
    ("zeta", 'ζ', Ord),
    ("eta", 'η', Ord),
    ("theta", 'θ', Ord),
    ("vartheta", 'ϑ', Ord),
    ("iota", 'ι', Ord),
    ("kappa", 'κ', Ord),
    ("varkappa", 'ϰ', Ord),
    ("lambda", 'λ', Ord),
    ("mu", 'μ', Ord),
    ("nu", 'ν', Ord),
    ("xi", 'ξ', Ord),
    ("omicron", 'ο', Ord),
    ("pi", 'π', Ord),
    ("varpi", 'ϖ', Ord),
    ("rho", 'ρ', Ord),
    ("varrho", 'ϱ', Ord),
    ("sigma", 'σ', Ord),
    ("varsigma", 'ς', Ord),
    ("tau", 'τ', Ord),
    ("upsilon", 'υ', Ord),
    ("phi", 'ϕ', Ord),
    ("varphi", 'φ', Ord),
    ("chi", 'χ', Ord),
    ("psi", 'ψ', Ord),
    ("omega", 'ω', Ord),
    ("digamma", 'ϝ', Ord),
    // Greek, upper case.
    ("Gamma", 'Γ', Ord),
    ("Delta", 'Δ', Ord),
    ("Theta", 'Θ', Ord),
    ("Lambda", 'Λ', Ord),
    ("Xi", 'Ξ', Ord),
    ("Pi", 'Π', Ord),
    ("Sigma", 'Σ', Ord),
    ("Upsilon", 'Υ', Ord),
    ("Phi", 'Φ', Ord),
    ("Psi", 'Ψ', Ord),
    ("Omega", 'Ω', Ord),
    ("varGamma", 'Γ', Ord),
    ("varDelta", 'Δ', Ord),
    ("varTheta", 'Θ', Ord),
    ("varLambda", 'Λ', Ord),
    ("varXi", 'Ξ', Ord),
    ("varPi", 'Π', Ord),
    ("varSigma", 'Σ', Ord),
    ("varUpsilon", 'Υ', Ord),
    ("varPhi", 'Φ', Ord),
    ("varPsi", 'Ψ', Ord),
    ("varOmega", 'Ω', Ord),
    // Relations.
    ("le", '≤', Rel),
    ("leq", '≤', Rel),
    ("ge", '≥', Rel),
    ("geq", '≥', Rel),
    ("leqslant", '⩽', Rel),
    ("geqslant", '⩾', Rel),
    ("lt", '<', Rel),
    ("gt", '>', Rel),
    ("ne", '≠', Rel),
    ("neq", '≠', Rel),
    ("approx", '≈', Rel),
    ("approxeq", '≊', Rel),
    ("equiv", '≡', Rel),
    ("sim", '∼', Rel),
    ("simeq", '≃', Rel),
    ("cong", '≅', Rel),
    ("asymp", '≍', Rel),
    ("doteq", '≐', Rel),
    ("propto", '∝', Rel),
    ("in", '∈', Rel),
    ("notin", '∉', Rel),
    ("ni", '∋', Rel),
    ("owns", '∋', Rel),
    ("subset", '⊂', Rel),
    ("subseteq", '⊆', Rel),
    ("subsetneq", '⊊', Rel),
    ("supset", '⊃', Rel),
    ("supseteq", '⊇', Rel),
    ("supsetneq", '⊋', Rel),
    ("sqsubseteq", '⊑', Rel),
    ("sqsupseteq", '⊒', Rel),
    ("mid", '∣', Rel),
    ("nmid", '∤', Rel),
    ("parallel", '∥', Rel),
    ("nparallel", '∦', Rel),
    ("perp", '⊥', Rel),
    ("prec", '≺', Rel),
    ("succ", '≻', Rel),
    ("preceq", '⪯', Rel),
    ("succeq", '⪰', Rel),
    ("ll", '≪', Rel),
    ("gg", '≫', Rel),
    ("lesssim", '≲', Rel),
    ("gtrsim", '≳', Rel),
    ("models", '⊨', Rel),
    ("vdash", '⊢', Rel),
    ("dashv", '⊣', Rel),
    ("vDash", '⊨', Rel),
    ("bowtie", '⋈', Rel),
    ("smile", '⌣', Rel),
    ("frown", '⌢', Rel),
    ("triangleq", '≜', Rel),
    ("coloneqq", '≔', Rel),
    ("eqqcolon", '≕', Rel),
    ("nless", '≮', Rel),
    ("ngtr", '≯', Rel),
    ("nleq", '≰', Rel),
    ("ngeq", '≱', Rel),
    ("nsim", '≁', Rel),
    ("ncong", '≇', Rel),
    ("nsubseteq", '⊈', Rel),
    ("nsupseteq", '⊉', Rel),
    // Arrows (relations).
    ("to", '→', Rel),
    ("rightarrow", '→', Rel),
    ("leftarrow", '←', Rel),
    ("gets", '←', Rel),
    ("leftrightarrow", '↔', Rel),
    ("Rightarrow", '⇒', Rel),
    ("Leftarrow", '⇐', Rel),
    ("Leftrightarrow", '⇔', Rel),
    ("longrightarrow", '⟶', Rel),
    ("longleftarrow", '⟵', Rel),
    ("longleftrightarrow", '⟷', Rel),
    ("Longrightarrow", '⟹', Rel),
    ("Longleftarrow", '⟸', Rel),
    ("Longleftrightarrow", '⟺', Rel),
    ("implies", '⟹', Rel),
    ("impliedby", '⟸', Rel),
    ("iff", '⟺', Rel),
    ("mapsto", '↦', Rel),
    ("longmapsto", '⟼', Rel),
    ("hookrightarrow", '↪', Rel),
    ("hookleftarrow", '↩', Rel),
    ("uparrow", '↑', Rel),
    ("downarrow", '↓', Rel),
    ("updownarrow", '↕', Rel),
    ("Uparrow", '⇑', Rel),
    ("Downarrow", '⇓', Rel),
    ("Updownarrow", '⇕', Rel),
    ("nearrow", '↗', Rel),
    ("searrow", '↘', Rel),
    ("swarrow", '↙', Rel),
    ("nwarrow", '↖', Rel),
    ("rightharpoonup", '⇀', Rel),
    ("rightharpoondown", '⇁', Rel),
    ("leftharpoonup", '↼', Rel),
    ("leftharpoondown", '↽', Rel),
    ("rightleftharpoons", '⇌', Rel),
    ("leftrightarrows", '⇆', Rel),
    ("rightleftarrows", '⇄', Rel),
    ("rightrightarrows", '⇉', Rel),
    ("leftleftarrows", '⇇', Rel),
    ("twoheadrightarrow", '↠', Rel),
    ("twoheadleftarrow", '↞', Rel),
    ("rightsquigarrow", '⇝', Rel),
    ("leadsto", '⇝', Rel),
    ("nrightarrow", '↛', Rel),
    ("nleftarrow", '↚', Rel),
    ("nRightarrow", '⇏', Rel),
    ("nLeftarrow", '⇍', Rel),
    ("nLeftrightarrow", '⇎', Rel),
    // Binary operators.
    ("pm", '±', Bin),
    ("mp", '∓', Bin),
    ("times", '×', Bin),
    ("div", '÷', Bin),
    ("cdot", '⋅', Bin),
    ("ast", '∗', Bin),
    ("star", '⋆', Bin),
    ("circ", '∘', Bin),
    ("bullet", '∙', Bin),
    ("cup", '∪', Bin),
    ("cap", '∩', Bin),
    ("sqcup", '⊔', Bin),
    ("sqcap", '⊓', Bin),
    ("uplus", '⊎', Bin),
    ("wedge", '∧', Bin),
    ("land", '∧', Bin),
    ("vee", '∨', Bin),
    ("lor", '∨', Bin),
    ("oplus", '⊕', Bin),
    ("ominus", '⊖', Bin),
    ("otimes", '⊗', Bin),
    ("oslash", '⊘', Bin),
    ("odot", '⊙', Bin),
    ("setminus", '∖', Bin),
    ("smallsetminus", '∖', Bin),
    ("dagger", '†', Bin),
    ("ddagger", '‡', Bin),
    ("amalg", '⨿', Bin),
    ("wr", '≀', Bin),
    ("diamond", '⋄', Bin),
    ("bigtriangleup", '△', Bin),
    ("bigtriangledown", '▽', Bin),
    ("triangleleft", '◁', Bin),
    ("triangleright", '▷', Bin),
    ("lhd", '⊲', Bin),
    ("rhd", '⊳', Bin),
    ("unlhd", '⊴', Bin),
    ("unrhd", '⊵', Bin),
    ("bigcirc", '◯', Bin),
    // Ordinary symbols.
    ("infty", '∞', Ord),
    ("partial", '∂', Ord),
    ("nabla", '∇', Ord),
    ("forall", '∀', Ord),
    ("exists", '∃', Ord),
    ("nexists", '∄', Ord),
    ("neg", '¬', Ord),
    ("lnot", '¬', Ord),
    ("emptyset", '∅', Ord),
    ("varnothing", '∅', Ord),
    ("ell", 'ℓ', Ord),
    ("hbar", 'ℏ', Ord),
    ("hslash", 'ℏ', Ord),
    ("Re", 'ℜ', Ord),
    ("Im", 'ℑ', Ord),
    ("aleph", 'ℵ', Ord),
    ("beth", 'ℶ', Ord),
    ("gimel", 'ℷ', Ord),
    ("wp", '℘', Ord),
    ("imath", 'ı', Ord),
    ("jmath", 'ȷ', Ord),
    ("prime", '′', Ord),
    ("angle", '∠', Ord),
    ("measuredangle", '∡', Ord),
    ("triangle", '△', Ord),
    ("top", '⊤', Ord),
    ("bot", '⊥', Ord),
    ("surd", '√', Ord),
    ("flat", '♭', Ord),
    ("natural", '♮', Ord),
    ("sharp", '♯', Ord),
    ("clubsuit", '♣', Ord),
    ("diamondsuit", '♢', Ord),
    ("heartsuit", '♡', Ord),
    ("spadesuit", '♠', Ord),
    ("square", '□', Ord),
    ("Box", '□', Ord),
    ("blacksquare", '■', Ord),
    ("checkmark", '✓', Ord),
    ("backslash", '\\', Ord),
    ("vert", '|', Ord),
    ("Vert", '‖', Ord),
    ("|", '‖', Ord),
    ("degree", '°', Ord),
    ("dots", '…', Inner),
    ("ldots", '…', Inner),
    ("dotsc", '…', Inner),
    ("dotso", '…', Inner),
    ("cdots", '⋯', Inner),
    ("dotsb", '⋯', Inner),
    ("dotsm", '⋯', Inner),
    ("dotsi", '⋯', Inner),
    ("vdots", '⋮', Ord),
    ("ddots", '⋱', Inner),
    ("#", '#', Ord),
    ("%", '%', Ord),
    ("&", '&', Ord),
    ("$", '$', Ord),
    ("_", '_', Ord),
    // Delimiters as atoms.
    ("{", '{', Open),
    ("}", '}', Close),
    ("lbrace", '{', Open),
    ("rbrace", '}', Close),
    ("lbrack", '[', Open),
    ("rbrack", ']', Close),
    ("langle", '⟨', Open),
    ("rangle", '⟩', Close),
    ("lfloor", '⌊', Open),
    ("rfloor", '⌋', Close),
    ("lceil", '⌈', Open),
    ("rceil", '⌉', Close),
    ("lvert", '|', Open),
    ("rvert", '|', Close),
    ("lVert", '‖', Open),
    ("rVert", '‖', Close),
    ("lgroup", '⟮', Open),
    ("rgroup", '⟯', Close),
    // Punctuation.
    ("colon", ':', Punct),
    ("cdotp", '·', Punct),
    ("ldotp", '.', Punct),
];

/// Large operators: their symbol and limits behaviour.
const LARGE_OPS: &[(&str, char, Limits)] = &[
    ("sum", '∑', Limits::Display),
    ("prod", '∏', Limits::Display),
    ("coprod", '∐', Limits::Display),
    ("bigcup", '⋃', Limits::Display),
    ("bigcap", '⋂', Limits::Display),
    ("bigsqcup", '⨆', Limits::Display),
    ("biguplus", '⨄', Limits::Display),
    ("bigoplus", '⨁', Limits::Display),
    ("bigotimes", '⨂', Limits::Display),
    ("bigodot", '⨀', Limits::Display),
    ("bigvee", '⋁', Limits::Display),
    ("bigwedge", '⋀', Limits::Display),
    ("int", '∫', Limits::Never),
    ("iint", '∬', Limits::Never),
    ("iiint", '∭', Limits::Never),
    ("iiiint", '⨌', Limits::Never),
    ("oint", '∮', Limits::Never),
    ("oiint", '∯', Limits::Never),
    ("oiiint", '∰', Limits::Never),
];

/// Named operators: their name as set and limits behaviour.
const NAMED_OPS: &[(&str, &str, Limits)] = &[
    ("sin", "sin", Limits::Never),
    ("cos", "cos", Limits::Never),
    ("tan", "tan", Limits::Never),
    ("cot", "cot", Limits::Never),
    ("sec", "sec", Limits::Never),
    ("csc", "csc", Limits::Never),
    ("sinh", "sinh", Limits::Never),
    ("cosh", "cosh", Limits::Never),
    ("tanh", "tanh", Limits::Never),
    ("coth", "coth", Limits::Never),
    ("arcsin", "arcsin", Limits::Never),
    ("arccos", "arccos", Limits::Never),
    ("arctan", "arctan", Limits::Never),
    ("log", "log", Limits::Never),
    ("lg", "lg", Limits::Never),
    ("ln", "ln", Limits::Never),
    ("exp", "exp", Limits::Never),
    ("arg", "arg", Limits::Never),
    ("deg", "deg", Limits::Never),
    ("dim", "dim", Limits::Never),
    ("ker", "ker", Limits::Never),
    ("hom", "hom", Limits::Never),
    ("lim", "lim", Limits::Display),
    ("limsup", "lim sup", Limits::Display),
    ("liminf", "lim inf", Limits::Display),
    ("max", "max", Limits::Display),
    ("min", "min", Limits::Display),
    ("sup", "sup", Limits::Display),
    ("inf", "inf", Limits::Display),
    ("det", "det", Limits::Display),
    ("gcd", "gcd", Limits::Display),
    ("Pr", "Pr", Limits::Display),
    ("injlim", "inj lim", Limits::Display),
    ("projlim", "proj lim", Limits::Display),
];

/// Delimiter commands for `\left`, `\right`, `\middle` and `\big` (characters such as `(` are taken as they are).
const DELIMITERS: &[(&str, char)] = &[
    ("{", '{'),
    ("}", '}'),
    ("lbrace", '{'),
    ("rbrace", '}'),
    ("lbrack", '['),
    ("rbrack", ']'),
    ("langle", '⟨'),
    ("rangle", '⟩'),
    ("lfloor", '⌊'),
    ("rfloor", '⌋'),
    ("lceil", '⌈'),
    ("rceil", '⌉'),
    ("vert", '|'),
    ("lvert", '|'),
    ("rvert", '|'),
    ("Vert", '‖'),
    ("lVert", '‖'),
    ("rVert", '‖'),
    ("|", '‖'),
    ("backslash", '\\'),
    ("lgroup", '⟮'),
    ("rgroup", '⟯'),
    ("uparrow", '↑'),
    ("downarrow", '↓'),
    ("updownarrow", '↕'),
    ("Uparrow", '⇑'),
    ("Downarrow", '⇓'),
    ("Updownarrow", '⇕'),
];

/// Negated forms for `\not`: the relation and its slashed character.
const NEGATIONS: &[(char, char)] = &[
    ('=', '≠'),
    ('<', '≮'),
    ('>', '≯'),
    ('≤', '≰'),
    ('≥', '≱'),
    ('∈', '∉'),
    ('∋', '∌'),
    ('≡', '≢'),
    ('∼', '≁'),
    ('≃', '≄'),
    ('≈', '≉'),
    ('≅', '≇'),
    ('⊂', '⊄'),
    ('⊃', '⊅'),
    ('⊆', '⊈'),
    ('⊇', '⊉'),
    ('∣', '∤'),
    ('∥', '∦'),
    ('≺', '⊀'),
    ('≻', '⊁'),
    ('→', '↛'),
    ('←', '↚'),
    ('↔', '↮'),
    ('⇒', '⇏'),
    ('⇐', '⇍'),
    ('⇔', '⇎'),
    ('∃', '∄'),
    ('⊢', '⊬'),
    ('⊨', '⊭'),
];

/// The character and class a symbol command stands for: `symbol("le")` is `≤`, a relation. Large and named
/// operators are not symbols (see [`large_op`] and [`named_op`]).
#[must_use]
pub fn symbol(command: &str) -> Option<MathChar> {
    SYMBOLS
        .iter()
        .find(|(name, ..)| *name == command)
        .map(|&(_, ch, class)| MathChar { ch, class })
}

/// The symbol and limits behaviour of a large operator command (`sum` is `∑`, limits in display).
#[must_use]
pub fn large_op(command: &str) -> Option<(char, Limits)> {
    LARGE_OPS
        .iter()
        .find(|(name, ..)| *name == command)
        .map(|&(_, ch, limits)| (ch, limits))
}

/// The name as set and limits behaviour of a named operator command (`limsup` is `lim sup`).
#[must_use]
pub fn named_op(command: &str) -> Option<(&'static str, Limits)> {
    NAMED_OPS
        .iter()
        .find(|(name, ..)| *name == command)
        .map(|&(_, text, limits)| (text, limits))
}

/// The delimiter a command names, for `\left` and friends.
#[must_use]
pub fn delimiter_command(command: &str) -> Option<char> {
    DELIMITERS
        .iter()
        .find(|(name, _)| *name == command)
        .map(|&(_, ch)| ch)
}

/// Whether a character may be used as a delimiter as it is (`\left(`).
#[must_use]
pub const fn is_delimiter_char(ch: char) -> bool {
    matches!(
        ch,
        '(' | ')' | '[' | ']' | '|' | '/' | '⟨' | '⟩' | '⌊' | '⌋' | '⌈' | '⌉' | '‖'
    )
}

/// The slashed form of a relation, for `\not`.
#[must_use]
pub fn negated(ch: char) -> Option<char> {
    NEGATIONS.iter().find(|(c, _)| *c == ch).map(|&(_, n)| n)
}

/// The math character for a character typed as it is: `+` is a binary `+`, `-` the minus sign `−`, `(` opens,
/// `,` is punctuation; a Unicode symbol in the table (`≤` typed directly) takes its command's class; anything
/// else is ordinary.
#[must_use]
pub fn char_atom(ch: char) -> MathChar {
    let (ch, class) = match ch {
        '-' => ('−', Bin),
        '+' | '*' => (ch, Bin),
        '=' | '<' | '>' | ':' => (ch, Rel),
        '(' | '[' => (ch, Open),
        ')' | ']' | '!' | '?' => (ch, Close),
        ',' | ';' => (ch, Punct),
        _ if ch.is_ascii() => (ch, Ord),
        _ => (
            ch,
            SYMBOLS
                .iter()
                .find(|&&(_, c, _)| c == ch)
                .map_or(Ord, |&(_, _, class)| class),
        ),
    };
    MathChar { ch, class }
}

/// The limits behaviour of a large operator typed as its Unicode character (`∑` rather than `\sum`).
#[must_use]
pub fn large_op_char(ch: char) -> Option<Limits> {
    LARGE_OPS
        .iter()
        .find(|&&(_, c, _)| c == ch)
        .map(|&(_, _, limits)| limits)
}
