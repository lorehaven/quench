---
title: Supported TeX
parent: quench-tex
grand_parent: Crates
nav_order: 3
---

Everything below was read from `src/lexer.rs`, `src/parse.rs`, `src/symbols.rs` and `src/macros.rs`. The input is the TeX **between** the dollar signs. Anything not listed is `Error::Unsupported` naming the command (or `\begin{name}`) and its byte offset, so a page can show the source instead. Spacing inside math is TeX's: spaces in the source do not matter, except inside `\text`.

## Characters and categories

The categories are TeX's plain ones and never change (no `\catcode`): `{` `}` group, `^` `_` scripts, `&` alignment tab, `#1`…`#9` macro parameters, `$` math shift (only inside `\text`), `~` a tie (a space of 1/3 em), `%` a comment to the end of the line. Runs of white space are one space token, and spaces after a control word are dropped. A control sequence is `\` plus ASCII letters, or `\` plus one other character (`\,`, `\{`, `\\`).

Typed characters and their classes:

| Typed | Becomes | Class |
|---|---|---|
| `-` | `−` (U+2212) | Bin |
| `+`, `*` | as typed | Bin |
| `=`, `<`, `>`, `:` | as typed | Rel |
| `(`, `[` | as typed | Open |
| `)`, `]`, `!`, `?` | as typed | Close |
| `,`, `;` | as typed | Punct |
| `'` | `′`, a prime (`f'`, `f''` = `f″`; see scripts) | Ord |
| other ASCII (letters, digits, `.`, `/`, the vertical bar, `@`, ...) | as typed | Ord |
| a Unicode symbol from the tables below (`≤` typed directly) | as typed | its command's class |
| a large operator symbol typed (`∑`, `∫`, ...) | an operator | Op, with that operator's limits |
| any other Unicode character | as typed | Ord |
| `#` not followed by 1-9 | | syntax error |
| `$`, `&` outside `\text` or an array | | syntax error |

A binary operator with nothing to operate on (`-x`, `(+1)`, `a = -b`) stays `Bin` in the tree and is spaced as ordinary by both outputs (TeX's rules 5 and 6).

## Symbols

All `Ord` unless the heading says otherwise.

**Greek, lower case**: `\alpha` α, `\beta` β, `\gamma` γ, `\delta` δ, `\epsilon` ϵ, `\varepsilon` ε, `\zeta` ζ, `\eta` η, `\theta` θ, `\vartheta` ϑ, `\iota` ι, `\kappa` κ, `\varkappa` ϰ, `\lambda` λ, `\mu` μ, `\nu` ν, `\xi` ξ, `\omicron` ο, `\pi` π, `\varpi` ϖ, `\rho` ρ, `\varrho` ϱ, `\sigma` σ, `\varsigma` ς, `\tau` τ, `\upsilon` υ, `\phi` ϕ, `\varphi` φ, `\chi` χ, `\psi` ψ, `\omega` ω, `\digamma` ϝ.

**Greek, upper case**: `\Gamma` Γ, `\Delta` Δ, `\Theta` Θ, `\Lambda` Λ, `\Xi` Ξ, `\Pi` Π, `\Sigma` Σ, `\Upsilon` Υ, `\Phi` Φ, `\Psi` Ψ, `\Omega` Ω, and `\varGamma` … `\varOmega` for the same eleven (the same characters; in the typesetter `\mathit` makes capitals italic).

**Relations (Rel)**: `\le` `\leq` ≤, `\ge` `\geq` ≥, `\leqslant` ⩽, `\geqslant` ⩾, `\lt` <, `\gt` >, `\ne` `\neq` ≠, `\approx` ≈, `\approxeq` ≊, `\equiv` ≡, `\sim` ∼, `\simeq` ≃, `\cong` ≅, `\asymp` ≍, `\doteq` ≐, `\propto` ∝, `\in` ∈, `\notin` ∉, `\ni` `\owns` ∋, `\subset` ⊂, `\subseteq` ⊆, `\subsetneq` ⊊, `\supset` ⊃, `\supseteq` ⊇, `\supsetneq` ⊋, `\sqsubseteq` ⊑, `\sqsupseteq` ⊒, `\mid` ∣, `\nmid` ∤, `\parallel` ∥, `\nparallel` ∦, `\perp` ⊥, `\prec` ≺, `\succ` ≻, `\preceq` ⪯, `\succeq` ⪰, `\ll` ≪, `\gg` ≫, `\lesssim` ≲, `\gtrsim` ≳, `\models` `\vDash` ⊨, `\vdash` ⊢, `\dashv` ⊣, `\bowtie` ⋈, `\smile` ⌣, `\frown` ⌢, `\triangleq` ≜, `\coloneqq` ≔, `\eqqcolon` ≕, `\nless` ≮, `\ngtr` ≯, `\nleq` ≰, `\ngeq` ≱, `\nsim` ≁, `\ncong` ≇, `\nsubseteq` ⊈, `\nsupseteq` ⊉.

**Arrows (Rel)**: `\to` `\rightarrow` →, `\leftarrow` `\gets` ←, `\leftrightarrow` ↔, `\Rightarrow` ⇒, `\Leftarrow` ⇐, `\Leftrightarrow` ⇔, `\longrightarrow` ⟶, `\longleftarrow` ⟵, `\longleftrightarrow` ⟷, `\Longrightarrow` `\implies` ⟹, `\Longleftarrow` `\impliedby` ⟸, `\Longleftrightarrow` `\iff` ⟺, `\mapsto` ↦, `\longmapsto` ⟼, `\hookrightarrow` ↪, `\hookleftarrow` ↩, `\uparrow` ↑, `\downarrow` ↓, `\updownarrow` ↕, `\Uparrow` ⇑, `\Downarrow` ⇓, `\Updownarrow` ⇕, `\nearrow` ↗, `\searrow` ↘, `\swarrow` ↙, `\nwarrow` ↖, `\rightharpoonup` ⇀, `\rightharpoondown` ⇁, `\leftharpoonup` ↼, `\leftharpoondown` ↽, `\rightleftharpoons` ⇌, `\leftrightarrows` ⇆, `\rightleftarrows` ⇄, `\rightrightarrows` ⇉, `\leftleftarrows` ⇇, `\twoheadrightarrow` ↠, `\twoheadleftarrow` ↞, `\rightsquigarrow` `\leadsto` ⇝, `\nrightarrow` ↛, `\nleftarrow` ↚, `\nRightarrow` ⇏, `\nLeftarrow` ⇍, `\nLeftrightarrow` ⇎.

**Binary operators (Bin)**: `\pm` ±, `\mp` ∓, `\times` ×, `\div` ÷, `\cdot` ⋅, `\ast` ∗, `\star` ⋆, `\circ` ∘, `\bullet` ∙, `\cup` ∪, `\cap` ∩, `\sqcup` ⊔, `\sqcap` ⊓, `\uplus` ⊎, `\wedge` `\land` ∧, `\vee` `\lor` ∨, `\oplus` ⊕, `\ominus` ⊖, `\otimes` ⊗, `\oslash` ⊘, `\odot` ⊙, `\setminus` `\smallsetminus` ∖, `\dagger` †, `\ddagger` ‡, `\amalg` ⨿, `\wr` ≀, `\diamond` ⋄, `\bigtriangleup` △, `\bigtriangledown` ▽, `\triangleleft` ◁, `\triangleright` ▷, `\lhd` ⊲, `\rhd` ⊳, `\unlhd` ⊴, `\unrhd` ⊵, `\bigcirc` ◯.

**Ordinary symbols**: `\infty` ∞, `\partial` ∂, `\nabla` ∇, `\forall` ∀, `\exists` ∃, `\nexists` ∄, `\neg` `\lnot` ¬, `\emptyset` `\varnothing` ∅, `\ell` ℓ, `\hbar` `\hslash` ℏ, `\Re` ℜ, `\Im` ℑ, `\aleph` ℵ, `\beth` ℶ, `\gimel` ℷ, `\wp` ℘, `\imath` ı, `\jmath` ȷ, `\prime` ′, `\angle` ∠, `\measuredangle` ∡, `\triangle` △, `\top` ⊤, `\bot` ⊥, `\surd` √, `\flat` ♭, `\natural` ♮, `\sharp` ♯, `\clubsuit` ♣, `\diamondsuit` ♢, `\heartsuit` ♡, `\spadesuit` ♠, `\square` `\Box` □, `\blacksquare` ■, `\checkmark` ✓, `\backslash` \\, `\vert` \|, `\Vert` `\|` ‖, `\degree` °, `\vdots` ⋮, and the escapes `\#`, `\%`, `\&`, `\$`, `\_`.

**Dots (Inner)**: `\dots` `\ldots` `\dotsc` `\dotso` …, `\cdots` `\dotsb` `\dotsm` `\dotsi` ⋯, `\ddots` ⋱ (`\vdots` is ordinary).

**Delimiters as atoms**: Open `\{` `\lbrace` {, `\lbrack` [, `\langle` ⟨, `\lfloor` ⌊, `\lceil` ⌈, `\lvert` \|, `\lVert` ‖, `\lgroup` ⟮; Close `\}` `\rbrace` }, `\rbrack` ], `\rangle` ⟩, `\rfloor` ⌋, `\rceil` ⌉, `\rvert` \|, `\rVert` ‖, `\rgroup` ⟯.

**Punctuation (Punct)**: `\colon` :, `\cdotp` ·, `\ldotp` `.`.

**Negation**: `\not` followed by `=`, `<`, `>`, `\le`/`≤`, `\ge`, `\in`, `\ni`, `\equiv`, `\sim`, `\simeq`, `\approx`, `\cong`, `\subset`, `\supset`, `\subseteq`, `\supseteq`, `\mid`, `\parallel`, `\prec`, `\succ`, `\to`, `\leftarrow`, `\leftrightarrow`, `\Rightarrow`, `\Leftarrow`, `\Leftrightarrow`, `\exists`, `\vdash`, `\models` gives the slashed character (≠ ≮ ≯ ≰ ≱ ∉ ∌ ≢ ≁ ≄ ≉ ≇ ⊄ ⊅ ⊈ ⊉ ∤ ∦ ⊀ ⊁ ↛ ↚ ↮ ⇏ ⇍ ⇎ ∄ ⊬ ⊭) with the original class. Anything else after `\not` (`\not\alpha`) is `Unsupported { command: "\\not" }`.

## Operators

**Large operators** (`Node::Op` with `OpName::Symbol`): limits above and below in display style (`Limits::Display`): `\sum` ∑, `\prod` ∏, `\coprod` ∐, `\bigcup` ⋃, `\bigcap` ⋂, `\bigsqcup` ⨆, `\biguplus` ⨄, `\bigoplus` ⨁, `\bigotimes` ⨂, `\bigodot` ⨀, `\bigvee` ⋁, `\bigwedge` ⋀. Limits beside (`Limits::Never`): `\int` ∫, `\iint` ∬, `\iiint` ∭, `\iiiint` ⨌, `\oint` ∮, `\oiint` ∯, `\oiiint` ∰. The same characters typed directly are the same operators.

**Named operators** (`OpName::Named`, set upright): limits beside: `\sin`, `\cos`, `\tan`, `\cot`, `\sec`, `\csc`, `\sinh`, `\cosh`, `\tanh`, `\coth`, `\arcsin`, `\arccos`, `\arctan`, `\log`, `\lg`, `\ln`, `\exp`, `\arg`, `\deg`, `\dim`, `\ker`, `\hom`. Limits in display: `\lim`, `\limsup` (lim sup), `\liminf` (lim inf), `\max`, `\min`, `\sup`, `\inf`, `\det`, `\gcd`, `\Pr`, `\injlim` (inj lim), `\projlim` (proj lim).

* `\operatorname{name}` (limits beside) and `\operatorname*{name}` (limits in display): the argument may hold letters, digits, `-`, spacing (a positive space becomes one space in the name), groups, `\mathrm`-style fonts, class changes and `\text`; anything else is ``Syntax: `\operatorname` takes a name of letters``.
* `\limits`, `\nolimits`, `\displaylimits` directly after an operator change its `Limits`: a built-in operator, `\operatorname`/`\operatorname*`, a `\DeclareMathOperator` macro, or `\mathop{…}` (whose `limits` field starts as `Limits::Display`, so it takes its scripts above and below in display style unless told otherwise). After anything else (`\mathrel{…}`, a letter) they are ``Syntax: `\limits` must follow an operator``. Several in a row: the last wins.

## Scripts and primes

`x^2`, `x_i`, `x_i^2` and `x^2_i` (either order), `{}^{14}C`, `^2` with no base (an empty group base). A script's argument is one token or a braced group (`x^\alpha`, `x^{n+1}`, `x^\frac12`). Primes: `f'`, `f''`, `f'''` are a superscript list starting with `′`, `″`, `‴` (`⁗` for four), and `f'^2` puts `2` after the primes in the same superscript. Errors: `x^2^3` (``double superscript``), `x_1_2` (``double subscript``), `x^2'` (a prime after a superscript is a double superscript), `x^^2` and `x^_2` (``a script right after a script``), `x^` (``missing argument for `\^` ``).

## Fractions, radicals, delimiters

| Command | Tree |
|---|---|
| `\frac{a}{b}` | `Frac { bar: true, style: None }` |
| `\dfrac`, `\cfrac` | `style: Some(Display)` (`\cfrac` is read as `\dfrac`: no continued-fraction layout) |
| `\tfrac` | `style: Some(Text)` |
| `\binom{n}{k}`, `\dbinom`, `\tbinom` | `bar: false`, `open: Some('(')`, `close: Some(')')`, style `None`/`Display`/`Text` |
| `\sqrt{x}`, `\sqrt[n]{x}` | `Sqrt { degree, body }`; `\sqrt[3]x` is fine |
| `\left( … \right)` | `Fenced`; a `\middle` inside gives `Middle` |
| `\big(`, `\Big(`, `\bigg(`, `\Bigg(` and the `l`, `r`, `m` forms (`\bigl(`, `\Bigr]`, `\biggm\vert`) | `Big { delim, size: 1..=4, class }` |

Arguments are one token or a braced group (`\frac12`, `\binom n k`). Infix forms (`\over`, `\atop`, `\choose`, `\brack`) are not supported.

**Delimiters** after `\left`, `\right`, `\middle` and `\big…`: `.` (none), `(` `)` `[` `]` `|` `/` `⟨` `⟩` `⌊` `⌋` `⌈` `⌉` `‖`, `<` and `>` (read as ⟨ and ⟩), and the commands `\{` `\}` `\lbrace` `\rbrace` `\lbrack` `\rbrack` `\langle` `\rangle` `\lfloor` `\rfloor` `\lceil` `\rceil` `\vert` `\lvert` `\rvert` `\Vert` `\lVert` `\rVert` `\|` `\backslash` `\lgroup` `\rgroup` `\uparrow` `\downarrow` `\updownarrow` `\Uparrow` `\Downarrow` `\Updownarrow`. Anything else is ``Syntax: `\left` needs a delimiter after it`` (with the command's name).

## Accents, braces, stacks

* Accents (`Node::Accent`): `\hat`, `\check`, `\tilde`, `\acute`, `\grave`, `\dot`, `\ddot`, `\dddot`, `\breve`, `\bar`, `\vec`, `\mathring`; wide: `\widehat`, `\widetilde`, `\widecheck`, `\overrightarrow`, `\overleftarrow`, `\overleftrightarrow`.
* `\overline{…}`, `\underline{…}`.
* `\overbrace{…}^{label}`, `\underbrace{…}_{label}`: the label is the `^` (over) or `_` (under) right after the brace, optional.
* `\overset{over}{base}`, `\underset{under}{base}`, `\stackrel{over}{base}` (`\stackrel` is a relation).

## Alphabets, text, styles, classes, colour

* **Math alphabets** with an argument: `\mathbb` `\Bbb`, `\mathcal`, `\mathscr`, `\mathfrak`, `\mathbf`, `\mathit`, `\mathrm` `\mathup`, `\mathsf`, `\mathtt`, `\boldsymbol` `\bm` `\pmb`. **Switches** holding the rest of the group: `\rm`, `\bf`, `\it`, `\sf`, `\tt`, `\cal` (`{\rm d}x`).
* **Text**: `\text`, `\textrm`, `\textnormal`, `\textup`, `\mbox`, `\hbox` (normal), `\textbf`, `\textit`, `\emph`, `\textsf`, `\texttt`; one character or a braced group. Inside: characters and spaces as typed (a run of spaces is one), `~` a space, nested groups, nested text font commands, `$…$` math (in text style), the escapes `\{` `\}` `\$` `\%` `\&` `\#` `\_`, `\textbackslash`, `\textasciitilde`, `\textasciicircum`, and the spacing commands below. Any other command inside text (`\text{\alpha}`, `\\`) is `Unsupported`; `^` or `_` in text is ``a script in text (use `$…$`)``.
* **Styles** holding the rest of the group: `\displaystyle`, `\textstyle`, `\scriptstyle`, `\scriptscriptstyle`.
* **Classes**: `\mathord`, `\mathop`, `\mathbin`, `\mathrel`, `\mathopen`, `\mathclose`, `\mathpunct`, `\mathinner` with an argument.

### Colour

`\color{name}` holds the rest of the group, `\textcolor{name}{…}` its argument; both take an optional model, `\color[rgb]{1,0.5,0}`, stored as `"rgb:1,0.5,0"`. The reader keeps the colour text as written (its argument must be plain characters; spaces are dropped); the typesetter resolves it (the names and models are in the [API reference](api.md#colours)) and reports an unknown one as `UnknownColour`; the text output ignores colour.

Hex colours are read verbatim in any form, `#rgb`, `#rrggbb` or `#rrggbbaa`, starting with a letter or a digit (`\color{#1a2b3c}`, `\textcolor{#123}{x}`): the tokenizer reads `#` and a digit as a macro parameter, and the colour argument turns it back into the two characters. Inside a definition, `#1` is still the macro's parameter (`\newcommand{\paint}[2]{\textcolor{#1}{#2}}` takes the colour as its first argument), so a hex colour that starts with a digit cannot be written literally in a macro body without arguments: `\newcommand{\c}{\color{#123456}}` is ``the body uses #1 but the macro takes 0 arguments``, as in TeX, which needs `##` there (not supported); use `\color[HTML]{123456}` in a body.

## Spacing

| Command | `Space` |
|---|---|
| `\,` `\thinspace` | `Mu(3.0)` |
| `\:` `\>` `\medspace` | `Mu(4.0)` |
| `\;` `\thickspace` | `Mu(5.0)` |
| `\!` `\negthinspace` | `Mu(-3.0)` |
| `\negmedspace` | `Mu(-4.0)` |
| `\negthickspace` | `Mu(-5.0)` |
| `\ ` (backslash space) `\space`, `~` | `Em(1/3)` |
| `\enspace` | `Em(0.5)` |
| `\quad` | `Em(1.0)` |
| `\qquad` | `Em(2.0)` |
| `\hspace{len}` `\hspace*{len}` `\hskip` `\kern` `\mkern` `\mskip` `\mspace` | the length |

A length is `{2em}`, or for `\kern`-like commands written straight after (`\kern3mu`, `\mkern-2mu`). Units: `mu` (stays `Mu`), `em`, `ex` (0.43 em), and `pt`, `bp` (1.00375 pt), `pc` (12 pt), `px` (0.75 pt), `in` (72.27 pt), `cm` (28.45 pt), `mm` (2.845 pt) converted at 10 pt to the em. A missing number is 1 (`\hspace{em}`, `-em`). Anything else is ``Syntax: `\hspace` needs a length, not `…` ``. Glue (`plus`, `minus`) is not read.

## Environments

`\begin{name} … \end{name}`, cells separated by `&`, rows by `\\` or `\cr` (an optional `[length]` or `*` after `\\` is skipped), `\hline` at the start of a row (any environment):

| Environment | `ArrayKind` | Delimiters | Columns |
|---|---|---|---|
| `matrix` | `Matrix` | none | centred |
| `pmatrix`, `bmatrix`, `Bmatrix`, `vmatrix`, `Vmatrix` | `Matrix` | parentheses, brackets, braces, single bars, double bars | centred |
| `smallmatrix` | `SmallMatrix` | none | centred, script size |
| `cases` | `Cases` | `{` left | left |
| `rcases` | `Cases` | `}` right | left |
| `aligned`, `align`, `align*`, `split`, `alignedat{n}` (n ignored) | `Aligned` | none | right, left, right, left, ... |
| `gathered`, `gather`, `gather*`, `equation`, `equation*` | `Gathered` | none | centred |
| `array{spec}` | `Array` | none | alignments and rules from the spec; extra cells centred |

Other column types (`p{…}`, `@{…}`, `*{…}`) are ``Syntax: array columns other than `l`, `c`, `r` and `|` are not supported``. Other environments (`tabular`, `multline`, `eqnarray`, ...) are `Unsupported { command: "\\begin{tabular}" }`. Mismatched ends are ``Syntax: `\begin{matrix}` ended by `\end{pmatrix}` ``. `\\` outside any environment makes the whole formula a `gathered` array (as KaTeX does).

## Macros

* `\newcommand{\name}{body}`, `\newcommand\name{body}`, `\newcommand{\name}[n]{body}` (0 to 9 arguments), `\newcommand{\name}[n][default]{body}` (the first argument optional: `\name[x]{…}` or `\name{…}`), `\newcommand*`; `\renewcommand` the same (no check that it existed); `\providecommand` defines only when the name is not defined yet.
* `\def\name{body}`, `\def\name#1#2{body}` (parameters in order), `\gdef` the same. Delimited parameters (`\def\a#1.{…}`) are `Unsupported { command: "\\def" }`.
* `\DeclareMathOperator{\name}{text}` and `\DeclareMathOperator*` define `\name` as `\operatorname{text}` (`\operatorname*`).
* Definitions apply from where they are made to the end of the formula, and, with `parse_with`, to the formulas after it. A macro shadows a built-in command of the same name. Arguments are one token or a braced group. There is no `\let`, `\edef`, `\expandafter`, `\catcode`, `\makeatletter`, or conditionals.

## Not supported

Not in the tables, so `Unsupported` (examples checked): `\over`, `\atop`, `\choose`, `\substack`, `\xrightarrow` and the other extensible arrows with arguments, `\phantom` and `\smash`, `\boxed`, `\tag`, `\label`, `\ref`, `\sideset`, `\let`, `\catcode`, `\href`, `\cancel`, `\mathchoice`, `\multicolumn`, environments other than the ones above. There is no `\cfrac` layout (it is `\dfrac`) and no skewed fractions.

## Errors

`Error::Unsupported { command, at }`: an unknown command (in math or text), an unknown environment (`\begin{name}`), `\not` before something without a negated form, a delimited `\def`.

`Error::Syntax { at, message }`, with these messages:

| Message | When |
|---|---|
| ``a lone `\` at the end`` | the source ends in `\` |
| `` unbalanced braces: `{` without `}` `` / `` `}` without `{` `` | |
| `` missing argument for `\frac` `` (the command's name) | an argument is missing or is a `}`, `&`, `\\`, `\end`, `\right`, `^`, `_` |
| `double superscript`, `double subscript`, `a script right after a script` | |
| `` `&` outside an array ``, `` `\\` outside an array `` | the latter inside a group or argument (`{a \\ b}`); at the top level `\\` makes a `gathered` array |
| `` `\end` without `\begin` ``, `` `\begin{x}` without `\end{x}` ``, `` `\begin{x}` ended by `\end{y}` `` | |
| `` `\right` without `\left` ``, `` `\left` without `\right` ``, `` `\middle` outside `\left`…`\right` `` | |
| `` `\left` needs a delimiter after it `` (also `\right`, `\middle`, `\big`…) | |
| `` `\sqrt[` without `]` ``, `` `[` without `]` ``, ``unexpected `]` `` | |
| `` `\hline` outside an array `` | `\hline` not at the start of a row |
| `` `\limits` must follow an operator `` | after something that is not an operator or a `\mathop{…}` |
| `` `\operatorname` takes a name of letters `` | |
| `` `\hspace` needs a length, not `x` `` | |
| `` `\color` takes plain characters `` (also `\begin`, `\end`) | a word argument with a command, group or parameter in it |
| `array columns other than …` (quoted in full under environments) | a column type other than the four |
| `` a `$` inside math ``, `` a `$` without its closing `$` ``, ``a script in text (use `$…$`)`` | |
| ``a `#` that is not a macro parameter (`#1` to `#9`)``, `a macro parameter outside a definition` | |
| `` `\abs` takes 0 to 9 arguments, not `10` `` | `Macros::define` with `params` over 9 |
| `` `\newcommand` needs the `\name` it defines ``, `` `\newcommand` takes 0 to 9 arguments, not `x` ``, `an optional argument's default for a macro without arguments`, `the body uses #2 but the macro takes 1 arguments`, `` `\def` needs the `\name` it defines `` | bad definitions |
| ``macro expansion too deep (is `\a` defined in terms of itself?)`` | more than 10 000 expansions in one formula |
| ``macro expansion too large (in `\a`)`` | more than 100 000 tokens waiting to be read |
| `the formula is nested too deeply` | more than 150 nested groups, arguments or atoms |
| `unexpected end of the formula`, `unexpected token` | |
