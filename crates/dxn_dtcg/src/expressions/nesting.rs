//! A bound on how deeply a JavaScript or TypeScript source nests, checked
//! before oxc parses it.
//!
//! oxc parses by recursive descent, and analyses and transforms the syntax
//! tree by recursive walks, all on the caller's stack: text such as ten
//! thousand nested parentheses, or a ten-thousand-term `+` chain, would
//! overflow it and abort the process. This pass reads the source as tokens
//! and bounds the depth of its syntax tree in units of about a kilobyte of
//! unoptimized oxc stack: every open bracket (`(`, `[`, `{`, `${`) costs
//! [`LEVEL_UNITS`], a type-argument `<` [`ANGLE_UNITS`] more, `function`,
//! `class` and `=>` [`FUNCTION_UNITS`], every other token that adds a node
//! above its operands [`TOKEN_UNITS`] (operators, member and call suffixes,
//! nesting keywords such as `new`, `typeof` and `await`), and a statement
//! keyword whose body may be another statement (`if`, `else`, `while`, …)
//! one unit. The weights come from measuring how deep each construct can go
//! before it overflows a 1 MiB stack, unoptimized: every accepted source
//! stays below half of that. A chain resets only where its subtree ends: at `,` and `;`,
//! at statement keywords that cannot appear inside an expression, and at an
//! automatic semicolon; statement keywords also survive a `;` followed by
//! `else`. A source fails when the bound exceeds [`MAX_UNITS`], far above
//! what real code uses.
//!
//! Whether a `/` divides or starts a regular expression depends on the
//! grammar after `)`, `}` and `of`; there the pass follows both readings and
//! keeps the deeper bound, so no reading can hide nesting inside what the
//! other takes for a string or a comment. Regular expression literals nest at
//! most [`MAX_REGEX_NESTING`] groups. A source no reading accepts is checked
//! against a coarse bound (every character that may start a token costs the
//! most any token costs), which also lets short sources skip the pass.

use std::collections::BTreeMap;

use super::compile::{CompileError, position_at};

/// The deepest syntax tree a source may have, in units.
pub(crate) const MAX_UNITS: u32 = 600;
/// The units of an open bracket.
const LEVEL_UNITS: u32 = 3;
/// The units of a token that adds a node above its operands.
const TOKEN_UNITS: u32 = 2;
/// The units of `function`, `class` and `=>`, whose bodies oxc's walks
/// enter with large frames.
const FUNCTION_UNITS: u32 = 5;
/// The units of a type-argument `<`, which nests oxc's type parser.
const ANGLE_UNITS: u32 = 3;
/// The most units one token adds: an opening bracket's level and the node
/// it forms in its parent.
const MAX_TOKEN_UNITS: u32 = LEVEL_UNITS + TOKEN_UNITS;
/// The deepest group nesting of a regular expression literal: oxc's
/// regular expression parser takes the most stack per level.
const MAX_REGEX_NESTING: u32 = 20;

/// Readings of an ambiguous source followed at once; real code needs a few.
const MAX_READINGS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Open {
    Paren,
    Bracket,
    Brace,
    Template,
}

#[derive(Clone, Debug)]
struct Level {
    open: Open,
    /// Whether this `(` is the head of `if`, `while`, `for` or `with`.
    head: bool,
    /// Nesting operators and keywords of the current expression.
    chain: u32,
    /// Unclosed `<` of the current statement (type arguments).
    angles: u32,
    /// Statement keywords whose body is still being parsed.
    statements: u32,
    /// A statement ended: the next token resets `statements` unless it
    /// continues the statement (`else`, `while`, `catch`, `finally`).
    ended: bool,
}

impl Level {
    fn new(open: Open, head: bool) -> Self {
        Self {
            open,
            head,
            chain: 0,
            angles: 0,
            statements: 0,
            ended: false,
        }
    }

    fn end_expression(&mut self) {
        self.chain = 0;
        self.angles = 0;
    }
}

/// What the previous token allows next.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Prev {
    /// An expression starts: a `/` starts a regular expression and `-` is
    /// a prefix operator.
    Start,
    /// An operand ended: a `/` divides.
    Operand,
    /// `)`, `}` or `of`: a `/` may divide or start a regular expression.
    Close,
    /// The `)` of a statement head: an expression starts.
    HeadClose,
    /// `.` or `?.`: the next word is a property name.
    Dot,
}

#[derive(Clone, Debug)]
struct Reading {
    position: usize,
    levels: Vec<Level>,
    prev: Prev,
    /// A line break precedes the next token.
    newline: bool,
    /// The next `(` is a statement head.
    head_pending: bool,
}

impl Reading {
    fn level(&mut self) -> &mut Level {
        self.levels
            .last_mut()
            .expect("the source level is never closed")
    }

    fn depth(&self) -> u32 {
        self.levels.iter().fold(0_u32, |depth, level| {
            depth
                .saturating_add(LEVEL_UNITS)
                .saturating_add(level.chain)
                .saturating_add(level.angles.saturating_mul(ANGLE_UNITS))
                .saturating_add(level.statements)
        })
    }

    /// Whether `other` reads the rest of the source alike, so the two can
    /// merge into one reading with the deeper counts.
    fn same_shape(&self, other: &Self) -> bool {
        self.prev == other.prev
            && self.newline == other.newline
            && self.head_pending == other.head_pending
            && self.levels.len() == other.levels.len()
            && self
                .levels
                .iter()
                .zip(&other.levels)
                .all(|(a, b)| a.open == b.open && a.head == b.head)
    }

    fn merge(&mut self, other: &Self) {
        for (level, theirs) in self.levels.iter_mut().zip(&other.levels) {
            level.chain = level.chain.max(theirs.chain);
            level.angles = level.angles.max(theirs.angles);
            level.statements = level.statements.max(theirs.statements);
            level.ended &= theirs.ended;
        }
    }
}

/// Fails when `source` nests deeper than [`MAX_UNITS`].
pub(crate) fn check(source: &str) -> Result<(), CompileError> {
    let start = if source.starts_with("#!") {
        source.find('\n').unwrap_or(source.len())
    } else {
        0
    };
    if coarse_units(&source[start..]) <= u64::from(MAX_UNITS) {
        return Ok(());
    }
    let mut pending = BTreeMap::<usize, Vec<Reading>>::new();
    pending.insert(
        start,
        vec![Reading {
            position: start,
            levels: vec![Level::new(Open::Brace, false)],
            prev: Prev::Start,
            newline: false,
            head_pending: false,
        }],
    );
    let mut completed = false;
    let mut deepest = 0_u32;
    let mut first_rejection = None::<usize>;
    while let Some((position, readings)) = pending.pop_first() {
        let mut merged = Vec::<Reading>::new();
        for reading in readings {
            match merged.iter_mut().find(|other| other.same_shape(&reading)) {
                Some(other) => other.merge(&reading),
                None => merged.push(reading),
            }
        }
        if merged.len() > MAX_READINGS {
            return Err(too_deep(source, position));
        }
        for reading in merged {
            deepest = deepest.max(reading.depth());
            match advance(source, reading)? {
                Advance::End => completed = true,
                Advance::Next(next) if next.is_empty() => {
                    first_rejection.get_or_insert(position);
                }
                Advance::Next(next) => {
                    for next in next {
                        pending.entry(next.position).or_default().push(next);
                    }
                }
            }
        }
    }
    match first_rejection {
        // No reading accepts the source, which oxc then rejects too; it may
        // parse up to that point and somewhat beyond, bounded coarsely.
        Some(position) if !completed => {
            let rest = coarse_units(&source[position..]);
            if u64::from(deepest) + rest > u64::from(MAX_UNITS) {
                return Err(too_deep(source, position));
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn too_deep(source: &str, position: usize) -> CompileError {
    CompileError {
        message: "the source nests too deeply".into(),
        position: Some(position_at(source, position)),
    }
}

/// A bound on the units of any reading of `text`: every character that may
/// start a token costs the most a token costs.
fn coarse_units(text: &str) -> u64 {
    let mut tokens = 0_u64;
    let mut in_word = false;
    for character in text.chars() {
        let word = character == '_' || character == '$' || character.is_alphanumeric();
        if character.is_whitespace() {
            in_word = false;
        } else if word {
            if !in_word {
                tokens += 1;
            }
            in_word = true;
        } else {
            tokens += 1;
            in_word = false;
        }
    }
    tokens.saturating_mul(u64::from(MAX_TOKEN_UNITS))
}

/// The outcome of reading one token.
enum Advance {
    /// The reading reached the end of the source.
    End,
    /// The readings after the token: two where a `/` is ambiguous, none
    /// where this reading of the source is not JavaScript.
    Next(Vec<Reading>),
}

/// Words after which an operand continues the expression on the next line,
/// so no semicolon is inserted before them.
const CONTINUING: &[&str] = &["in", "instanceof", "as", "satisfies", "of", "extends", "is"];

/// Reads one token of `reading`.
fn advance(source: &str, mut reading: Reading) -> Result<Advance, CompileError> {
    let bytes = source.as_bytes();
    let Some(start) = skip_trivia(source, &mut reading) else {
        // The end, or an unterminated block comment.
        return Ok(
            if reading.position == source.len() || source[reading.position..].trim().is_empty() {
                Advance::End
            } else {
                Advance::Next(Vec::new())
            },
        );
    };
    let rest = &source[start..];
    let first = rest.chars().next().expect("not at the end");
    let token = if first == '/' {
        match reading.prev {
            Prev::Operand => Token::Punct(if rest.starts_with("/=") { "/=" } else { "/" }),
            Prev::Close => {
                let mut division = reading.clone();
                let mut successors = Vec::new();
                let operator = if rest.starts_with("/=") { "/=" } else { "/" };
                if apply(source, &mut division, start, Token::Punct(operator))? {
                    successors.push(division);
                }
                if let Some(end) = regex_end(source, start)?
                    && apply(source, &mut reading, start, Token::Literal(end))?
                {
                    successors.push(reading);
                }
                return Ok(Advance::Next(successors));
            }
            _ => match regex_end(source, start)? {
                Some(end) => Token::Literal(end),
                None => return Ok(Advance::Next(Vec::new())),
            },
        }
    } else if first == '\'' || first == '"' {
        match string_end(bytes, start) {
            Some(end) => Token::Literal(end),
            None => return Ok(Advance::Next(Vec::new())),
        }
    } else if first == '`' {
        Token::Template(start + 1)
    } else if first.is_ascii_digit()
        || (first == '.' && rest[1..].starts_with(|next: char| next.is_ascii_digit()))
    {
        let end = start
            + rest
                .find(|next: char| !(next.is_ascii_alphanumeric() || next == '_' || next == '.'))
                .unwrap_or(rest.len());
        Token::Literal(end)
    } else if is_word_start(first) {
        Token::Word(start + word_length(rest))
    } else if let Some(punct) = PUNCTUATORS.iter().find(|punct| rest.starts_with(**punct)) {
        Token::Punct(punct)
    } else {
        // An unknown character: oxc rejects it, so this reading ends.
        return Ok(Advance::Next(Vec::new()));
    };
    Ok(Advance::Next(
        if apply(source, &mut reading, start, token)? {
            vec![reading]
        } else {
            Vec::new()
        },
    ))
}

enum Token {
    /// An identifier or keyword ending at the offset.
    Word(usize),
    /// A number, string or regular expression ending at the offset.
    Literal(usize),
    /// A template literal whose text starts at the offset.
    Template(usize),
    Punct(&'static str),
}

/// Applies `token` (at `start`) to `reading`; `false` when this reading of
/// the source is not JavaScript there.
fn apply(
    source: &str,
    reading: &mut Reading,
    start: usize,
    token: Token,
) -> Result<bool, CompileError> {
    let word = match token {
        Token::Word(end) => Some(&source[start..end]),
        _ => None,
    };
    let property = reading.prev == Prev::Dot;
    // An automatic semicolon: an operand or a block ends a line (or a block
    // is followed by a word) and the next token cannot continue it.
    let starts_statement = match &token {
        Token::Word(_) => !word.is_some_and(|word| CONTINUING.contains(&word)),
        Token::Literal(_) => true,
        _ => false,
    };
    let after_block = reading.prev == Prev::Close && source[..start].trim_end().ends_with('}');
    if starts_statement
        && ((reading.newline && matches!(reading.prev, Prev::Operand | Prev::Close)) || after_block)
    {
        let level = reading.level();
        level.end_expression();
        level.ended = true;
    }
    let continues = word.is_some_and(|word| matches!(word, "else" | "while" | "catch" | "finally"));
    let level = reading.level();
    if level.ended {
        if !continues {
            level.statements = 0;
        }
        level.ended = false;
    }
    let head_pending = std::mem::take(&mut reading.head_pending);
    reading.newline = false;
    let next = match token {
        Token::Word(end) => {
            let word = &source[start..end];
            if property {
                Some((end, Prev::Operand))
            } else {
                let level = reading.level();
                let prev = match word {
                    "if" | "while" | "for" | "with" => {
                        level.statements += 1;
                        reading.head_pending = true;
                        Prev::Start
                    }
                    "else" | "do" => {
                        level.statements += 1;
                        Prev::Start
                    }
                    "await" if head_pending => {
                        reading.head_pending = true;
                        Prev::Start
                    }
                    "new" | "typeof" | "void" | "delete" | "await" | "yield" | "extends"
                    | "keyof" | "infer" | "readonly" | "unique" | "asserts" | "in"
                    | "instanceof" | "as" | "satisfies" | "is" | "import" => {
                        level.chain += TOKEN_UNITS;
                        Prev::Start
                    }
                    "function" | "class" => {
                        level.chain += FUNCTION_UNITS;
                        Prev::Start
                    }
                    "const" | "let" | "var" | "return" | "throw" | "break" | "continue"
                    | "debugger" | "export" | "case" | "default" => {
                        level.end_expression();
                        Prev::Start
                    }
                    "of" => {
                        level.chain += TOKEN_UNITS;
                        Prev::Close
                    }
                    _ => Prev::Operand,
                };
                Some((end, prev))
            }
        }
        Token::Literal(end) => Some((end, Prev::Operand)),
        Token::Template(text) => {
            reading.level().chain += TOKEN_UNITS;
            template(source, reading, text)
        }
        Token::Punct(punct) => {
            let end = start + punct.len();
            let prefix = matches!(reading.prev, Prev::Start | Prev::HeadClose);
            let level = reading.level();
            let prev = match punct {
                "(" | "[" | "{" => {
                    // The node the bracket forms (a call, an index, a
                    // literal) sits in this level's chain.
                    level.chain += TOKEN_UNITS;
                    let open = match punct {
                        "(" => Open::Paren,
                        "[" => Open::Bracket,
                        _ => Open::Brace,
                    };
                    let head = open == Open::Paren && head_pending;
                    reading.levels.push(Level::new(open, head));
                    Prev::Start
                }
                ")" | "]" | "}" => {
                    if reading.levels.len() == 1 {
                        return Ok(false);
                    }
                    let closed = reading.levels.pop().expect("an open level");
                    match (punct, closed.open) {
                        (")", Open::Paren) if closed.head => Prev::HeadClose,
                        (")", Open::Paren) => Prev::Close,
                        ("]", Open::Bracket) => Prev::Operand,
                        ("}", Open::Brace) => {
                            reading.level().ended = true;
                            Prev::Close
                        }
                        ("}", Open::Template) => {
                            let next = template(source, reading, end);
                            return finish(source, reading, next);
                        }
                        _ => return Ok(false),
                    }
                }
                "," => {
                    level.chain = 0;
                    Prev::Start
                }
                ";" => {
                    level.end_expression();
                    level.ended = true;
                    Prev::Start
                }
                "." | "?." => {
                    level.chain += TOKEN_UNITS;
                    Prev::Dot
                }
                "<" => {
                    level.chain += TOKEN_UNITS;
                    level.angles += 1;
                    Prev::Start
                }
                ">" | ">>" | ">>>" => {
                    level.chain += TOKEN_UNITS;
                    level.angles = level.angles.saturating_sub(punct.len() as u32);
                    Prev::Start
                }
                // After an operand, `++`, `--` and TypeScript's `!` are
                // postfix: an operand ends.
                "++" | "--" | "!" if !prefix => {
                    level.chain += TOKEN_UNITS;
                    Prev::Operand
                }
                "=>" => {
                    level.chain += FUNCTION_UNITS;
                    Prev::Start
                }
                _ => {
                    level.chain += TOKEN_UNITS;
                    Prev::Start
                }
            };
            Some((end, prev))
        }
    };
    finish(source, reading, next)
}

/// Moves `reading` past a token ending at `next.0` and checks its depth.
fn finish(
    source: &str,
    reading: &mut Reading,
    next: Option<(usize, Prev)>,
) -> Result<bool, CompileError> {
    let Some((end, prev)) = next else {
        return Ok(false);
    };
    if reading.depth() > MAX_UNITS {
        return Err(too_deep(source, reading.position));
    }
    reading.position = end;
    reading.prev = prev;
    Ok(true)
}

/// Reads template text from `text`: to the closing backtick (an operand),
/// or to a `${`, which opens a level.
fn template(source: &str, reading: &mut Reading, text: usize) -> Option<(usize, Prev)> {
    let bytes = source.as_bytes();
    let mut index = text;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'`' => return Some((index + 1, Prev::Operand)),
            b'$' if bytes.get(index + 1) == Some(&b'{') => {
                reading.levels.push(Level::new(Open::Template, false));
                return Some((index + 2, Prev::Start));
            }
            _ => index += 1,
        }
    }
    None
}

/// Skips whitespace and comments, noting line breaks; the offset of the
/// next token, or `None` at the end of the source.
fn skip_trivia(source: &str, reading: &mut Reading) -> Option<usize> {
    let mut index = reading.position;
    loop {
        let rest = &source[index..];
        let next = rest.chars().next()?;
        if matches!(next, '\n' | '\r' | '\u{2028}' | '\u{2029}') {
            reading.newline = true;
            index += next.len_utf8();
        } else if next.is_whitespace() || next == '\u{feff}' {
            index += next.len_utf8();
        } else if rest.starts_with("//") {
            index += rest.find(['\n', '\r']).unwrap_or(rest.len());
        } else if let Some(comment) = rest.strip_prefix("/*") {
            let end = comment.find("*/")? + 4;
            if rest[..end].contains(['\n', '\r', '\u{2028}', '\u{2029}']) {
                reading.newline = true;
            }
            index += end;
        } else {
            return Some(index);
        }
    }
}

/// The end of the string literal starting at `start`, or `None` when it is
/// unterminated on its line.
fn string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            // A line continuation may end in `\r\n`.
            b'\\' if bytes[index + 1..].starts_with(b"\r\n") => index += 3,
            b'\\' => index += 2,
            b'\n' | b'\r' => return None,
            byte if byte == quote => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// The end of the regular expression literal starting at `start` (with its
/// flags), or `None` when it is unterminated on its line. Fails when its
/// groups and classes nest deeper than [`MAX_REGEX_NESTING`]; a class nests
/// inside another only with the `v` flag, so both readings are tracked until
/// the flags are known.
fn regex_end(source: &str, start: usize) -> Result<Option<usize>, CompileError> {
    let bytes = source.as_bytes();
    let mut index = start + 1;
    let mut groups = 0_u32;
    // Classes as the `v` flag nests them, and without it (at most one).
    let (mut nested, mut flat) = (0_u32, 0_u32);
    let (mut deepest_nested, mut deepest_flat) = (0_u32, 0_u32);
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'\n' | b'\r' => return Ok(None),
            b'[' => {
                nested += 1;
                flat = 1;
                index += 1;
            }
            b']' => {
                nested = nested.saturating_sub(1);
                flat = 0;
                index += 1;
            }
            b'(' if flat == 0 => {
                groups += 1;
                index += 1;
            }
            b')' if flat == 0 => {
                groups = groups.saturating_sub(1);
                index += 1;
            }
            b'/' if flat == 0 => {
                index += 1;
                let flags = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                let deepest = if source[flags..index].contains('v') {
                    deepest_nested
                } else {
                    deepest_flat
                };
                if deepest > MAX_REGEX_NESTING {
                    return Err(too_deep(source, start));
                }
                return Ok(Some(index));
            }
            _ => index += 1,
        }
        deepest_nested = deepest_nested.max(groups + nested);
        deepest_flat = deepest_flat.max(groups + flat);
    }
    Ok(None)
}

fn is_word_start(first: char) -> bool {
    first == '_'
        || first == '$'
        || first == '#'
        || first == '\\'
        || first.is_alphabetic()
        || !first.is_ascii()
}

/// The byte length of the identifier or keyword at the start of `text`.
fn word_length(text: &str) -> usize {
    let mut length = 0;
    let mut chars = text.char_indices();
    while let Some((index, next)) = chars.next() {
        if next == '\\' {
            chars.next();
            length = index + 2;
            continue;
        }
        if index > 0 && !(next == '_' || next == '$' || next.is_alphanumeric() || !next.is_ascii())
        {
            return index;
        }
        length = index + next.len_utf8();
    }
    length
}

/// Punctuators, longest first.
const PUNCTUATORS: &[&str] = &[
    ">>>=", "...", "===", "!==", "**=", "<<=", ">>=", ">>>", "&&=", "||=", "??=", "=>", "==", "!=",
    "<=", ">=", "&&", "||", "??", "?.", "++", "--", "+=", "-=", "*=", "%=", "&=", "|=", "^=", "**",
    "<<", ">>", "{", "}", "(", ")", "[", "]", ";", ",", "<", ">", "+", "-", "*", "%", "&", "|",
    "^", "!", "~", "?", ":", "=", ".", "@",
];
