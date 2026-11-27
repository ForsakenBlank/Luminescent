//! The sum parser and evaluator. Nothing here runs text as code: a sum is cut into tokens, read into a small
//! tree with Python's precedence rules, and the tree is walked. Anything the calculator does not do (strings,
//! lists, comparisons, other calls) reads fine but answers "not allowed in a sum", as the Python version does,
//! and the limits on powers, shifts and factorials keep `9**9**9` from hanging the app.

use std::collections::HashMap;
use std::sync::LazyLock;

use num_bigint::BigInt;
use num_traits::{Num as _, One, Signed, ToPrimitive, Zero};
use regex::Regex;

use crate::{Error, Num};

pub const MAX_POW: u32 = 10_000;
pub const MAX_FACT: u32 = 1_000;
const MAX_SHIFT: u32 = 4_096;
const MAX_DEPTH: usize = 150;

pub const FUNCTIONS: &[&str] = &[
    "sqrt",
    "cbrt",
    "abs",
    "round",
    "floor",
    "ceil",
    "sin",
    "cos",
    "tan",
    "asin",
    "acos",
    "atan",
    "ln",
    "log",
    "log2",
    "exp",
    "fact",
    "factorial",
    "min",
    "max",
    "hypot",
];

pub const CONSTANTS: &[(&str, f64)] =
    &[("pi", std::f64::consts::PI), ("e", std::f64::consts::E), ("tau", std::f64::consts::TAU)];

/// Names a pin can not take: the functions, the constants, `ans` and `mod`.
pub fn is_reserved(name: &str) -> bool {
    FUNCTIONS.contains(&name) || CONSTANTS.iter().any(|(n, _)| *n == name) || name == "ans" || name == "mod"
}

/// What a sum can use besides its own numbers.
#[derive(Debug, Clone, Default)]
pub struct Scope {
    /// `ans` and the pins.
    pub variables: HashMap<String, Num>,
    /// Trig works in degrees (the default) or radians.
    pub degrees: bool,
}

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).expect("the pattern is written here and is valid")
}

/// Turns what was typed into what the parser reads (`_prepare`): the typographic signs, `1,000`, `54 + 10%`,
/// `50%` and the word operators.
pub fn prepare(text: &str) -> String {
    static PERCENT_OF: LazyLock<Regex> = LazyLock::new(|| regex(r"^(.+?)\s*([+-])\s*(\d+(?:\.\d+)?)\s*%$"));
    static PERCENT: LazyLock<Regex> = LazyLock::new(|| regex(r"(\d+(?:\.\d+)?|\))\s*%"));
    static MOD: LazyLock<Regex> = LazyLock::new(|| regex(r"\bmod\b"));
    static XOR: LazyLock<Regex> = LazyLock::new(|| regex(r"\bxor\b"));
    static AND: LazyLock<Regex> = LazyLock::new(|| regex(r"\band\b"));
    static OR: LazyLock<Regex> = LazyLock::new(|| regex(r"\bor\b"));

    let t = text.trim().replace('×', "*").replace('÷', "/").replace('−', "-").replace('^', "**");
    let mut t = drop_thousands_commas(&t);
    // 54 + 10% means 54 plus 10 % of it, like a phone
    if let Some(m) = PERCENT_OF.captures(&t) {
        t = format!("({})*(1{}{}/100)", &m[1], &m[2], &m[3]);
    }
    let t = PERCENT.replace_all(&t, "${1}/100");
    // `^` became `**` above, so a `^` after this is the word xor
    let t = MOD.replace_all(&t, "%");
    let t = XOR.replace_all(&t, "^");
    let t = AND.replace_all(&t, "&");
    OR.replace_all(&t, "|").into_owned()
}

/// `1,000,000` becomes `1000000`: a comma after a digit and before exactly three digits.
fn drop_thousands_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    for (i, &c) in chars.iter().enumerate() {
        let thousands = c == ','
            && i > 0
            && chars[i - 1].is_ascii_digit()
            && chars.get(i + 1..i + 4).is_some_and(|d| d.iter().all(char::is_ascii_digit))
            && !chars.get(i + 4).is_some_and(char::is_ascii_digit);
        if !thousands {
            out.push(c);
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(Num),
    /// Read fine but not a number the calculator knows (`1j`).
    Imaginary,
    Name(String),
    Str,
    Op(&'static str),
}

const OPERATORS: &[&str] = &[
    "**", "//", "<<", ">>", "<=", ">=", "==", "!=", "+", "-", "*", "/", "%", "&", "|", "^", "~", "(", ")",
    "[", "]", "{", "}", ",", ".", ":", "<", ">", "@",
];

fn syntax() -> Error {
    Error::Calc("not a sum".into())
}

fn tokens(src: &str) -> Result<Vec<Token>, Error> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) {
            let (token, next) = number(&chars, i)?;
            out.push(token);
            i = next;
        } else if c.is_alphabetic() || c == '_' {
            let end = chars[i..]
                .iter()
                .position(|c| !(c.is_alphanumeric() || *c == '_'))
                .map_or(chars.len(), |n| i + n);
            out.push(Token::Name(chars[i..end].iter().collect()));
            i = end;
        } else if c == '\'' || c == '"' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != c {
                j += if chars[j] == '\\' { 2 } else { 1 };
            }
            if j >= chars.len() {
                return Err(syntax());
            }
            out.push(Token::Str);
            i = j + 1;
        } else {
            let rest: String = chars[i..chars.len().min(i + 2)].iter().collect();
            let op = OPERATORS.iter().find(|op| rest.starts_with(**op)).ok_or_else(syntax)?;
            out.push(Token::Op(op));
            i += op.len();
        }
    }
    Ok(out)
}

fn digits_from(chars: &[char], mut i: usize, ok: impl Fn(char) -> bool) -> usize {
    while i < chars.len() && (ok(chars[i]) || (chars[i] == '_' && chars.get(i + 1).is_some_and(|n| ok(*n)))) {
        i += 1;
    }
    i
}

/// A number literal as Python reads it: `12`, `1_000`, `0x1F`, `0b101`, `0o17`, `2.5`, `.5`, `1e3`, `3j`.
fn number(chars: &[char], start: usize) -> Result<(Token, usize), Error> {
    let text = |a: usize, b: usize| -> String { chars[a..b].iter().filter(|c| **c != '_').collect() };
    if chars[start] == '0'
        && let Some(radix) = chars.get(start + 1).and_then(|c| match c.to_ascii_lowercase() {
            'x' => Some(16),
            'o' => Some(8),
            'b' => Some(2),
            _ => None,
        })
    {
        let end = digits_from(chars, start + 2, |c| c.is_digit(radix));
        if end == start + 2 || chars.get(end).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
            return Err(syntax());
        }
        let value = BigInt::from_str_radix(&text(start + 2, end), radix).map_err(|_| syntax())?;
        return Ok((Token::Number(Num::Int(value)), end));
    }
    let mut end = digits_from(chars, start, |c| c.is_ascii_digit());
    let mut is_float = false;
    if chars.get(end) == Some(&'.') {
        is_float = true;
        end = digits_from(chars, end + 1, |c| c.is_ascii_digit());
    }
    if matches!(chars.get(end), Some('e' | 'E')) {
        let mut j = end + 1;
        if matches!(chars.get(j), Some('+' | '-')) {
            j += 1;
        }
        if chars.get(j).is_some_and(char::is_ascii_digit) {
            is_float = true;
            end = digits_from(chars, j, |c| c.is_ascii_digit());
        }
    }
    let literal = text(start, end);
    let imaginary = matches!(chars.get(end), Some('j' | 'J'));
    let end_after = end + usize::from(imaginary);
    if chars.get(end_after).is_some_and(|c| c.is_alphanumeric() || *c == '_') {
        // `2pi`: a number straight into a name is a syntax error
        return Err(syntax());
    }
    if imaginary {
        return Ok((Token::Imaginary, end_after));
    }
    if is_float {
        let value: f64 = literal.parse().map_err(|_| syntax())?;
        return Ok((Token::Number(Num::Float(value)), end));
    }
    if literal.len() > 1 && literal.starts_with('0') && literal.chars().any(|c| c != '0') {
        return Err(syntax());
    }
    let value = literal.parse::<BigInt>().map_err(|_| syntax())?;
    Ok((Token::Number(Num::Int(value)), end))
}

#[derive(Debug)]
enum Node {
    Number(Num),
    Name(String),
    Binary(Binary, Box<Node>, Box<Node>),
    Negate(Box<Node>),
    Plus(Box<Node>),
    Invert(Box<Node>),
    Call(String, Vec<Node>),
    /// Valid Python, but not something a sum may contain.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Binary {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Mod,
    Pow,
    And,
    Or,
    Xor,
    Shl,
    Shr,
}

struct Parser {
    tokens: Vec<Token>,
    at: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.at)
    }

    fn peek_op(&self) -> Option<&'static str> {
        match self.peek() {
            Some(Token::Op(op)) => Some(op),
            _ => None,
        }
    }

    fn eat(&mut self, op: &str) -> bool {
        if self.peek_op() == Some(op) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, op: &str) -> Result<(), Error> {
        if self.eat(op) { Ok(()) } else { Err(syntax()) }
    }

    fn enter(&mut self) -> Result<(), Error> {
        self.depth += 1;
        if self.depth > MAX_DEPTH { Err(syntax()) } else { Ok(()) }
    }

    /// The whole text: one expression, or several with commas (which is a tuple, so not a sum).
    fn top(&mut self) -> Result<Node, Error> {
        let first = self.comparison()?;
        if self.peek_op() == Some(",") {
            while self.eat(",") {
                if self.at >= self.tokens.len() {
                    break;
                }
                self.comparison()?;
            }
            return Ok(Node::Other);
        }
        Ok(first)
    }

    fn comparison(&mut self) -> Result<Node, Error> {
        if matches!(self.peek(), Some(Token::Name(n)) if n == "not") {
            self.at += 1;
            self.comparison()?;
            return Ok(Node::Other);
        }
        let first = self.binary(0)?;
        let mut compared = false;
        while matches!(self.peek_op(), Some("<" | ">" | "<=" | ">=" | "==" | "!=")) {
            self.at += 1;
            self.binary(0)?;
            compared = true;
        }
        Ok(if compared { Node::Other } else { first })
    }

    /// Levels from loosest: `|`, `^`, `&`, shifts, `+ -`, `* / // % @`.
    fn binary(&mut self, level: usize) -> Result<Node, Error> {
        const LEVELS: &[&[(&str, Option<Binary>)]] = &[
            &[("|", Some(Binary::Or))],
            &[("^", Some(Binary::Xor))],
            &[("&", Some(Binary::And))],
            &[("<<", Some(Binary::Shl)), (">>", Some(Binary::Shr))],
            &[("+", Some(Binary::Add)), ("-", Some(Binary::Sub))],
            &[
                ("*", Some(Binary::Mul)),
                ("/", Some(Binary::Div)),
                ("//", Some(Binary::FloorDiv)),
                ("%", Some(Binary::Mod)),
                ("@", None),
            ],
        ];
        if level == LEVELS.len() {
            return self.factor();
        }
        self.enter()?;
        let mut left = self.binary(level + 1)?;
        while let Some(op) = self.peek_op() {
            let Some((_, kind)) = LEVELS[level].iter().find(|(name, _)| *name == op) else { break };
            self.at += 1;
            let right = self.binary(level + 1)?;
            left = match kind {
                Some(kind) => Node::Binary(*kind, Box::new(left), Box::new(right)),
                None => Node::Other,
            };
        }
        self.depth -= 1;
        Ok(left)
    }

    fn factor(&mut self) -> Result<Node, Error> {
        self.enter()?;
        let node = match self.peek_op() {
            Some("-") => {
                self.at += 1;
                Node::Negate(Box::new(self.factor()?))
            }
            Some("+") => {
                self.at += 1;
                Node::Plus(Box::new(self.factor()?))
            }
            Some("~") => {
                self.at += 1;
                Node::Invert(Box::new(self.factor()?))
            }
            _ => self.power()?,
        };
        self.depth -= 1;
        Ok(node)
    }

    /// `**` binds tighter than a minus on its left (`-2**2` is `-4`) but takes one on its right (`2**-1`).
    fn power(&mut self) -> Result<Node, Error> {
        let base = self.primary()?;
        if self.eat("**") {
            let exponent = self.factor()?;
            return Ok(Node::Binary(Binary::Pow, Box::new(base), Box::new(exponent)));
        }
        Ok(base)
    }

    fn primary(&mut self) -> Result<Node, Error> {
        self.enter()?;
        let token = self.tokens.get(self.at).cloned().ok_or_else(syntax)?;
        self.at += 1;
        let mut node = match token {
            Token::Number(n) => Node::Number(n),
            Token::Imaginary | Token::Str => Node::Other,
            Token::Name(name) => match name.as_str() {
                "True" | "False" | "None" => Node::Other,
                _ if is_keyword(&name) => return Err(syntax()),
                _ => Node::Name(name),
            },
            Token::Op("(") => {
                if self.eat(")") {
                    Node::Other
                } else {
                    let inner = self.top()?;
                    self.expect(")")?;
                    inner
                }
            }
            Token::Op("[") => self.display("]")?,
            Token::Op("{") => self.display("}")?,
            Token::Op(_) => return Err(syntax()),
        };
        loop {
            if self.eat("(") {
                node = self.call(node)?;
            } else if self.eat(".") {
                match self.tokens.get(self.at) {
                    Some(Token::Name(_)) => self.at += 1,
                    _ => return Err(syntax()),
                }
                node = Node::Other;
            } else if self.eat("[") {
                self.slice()?;
                node = Node::Other;
            } else {
                break;
            }
        }
        self.depth -= 1;
        Ok(node)
    }

    /// `[1, 2]` and `{1: 2}`: read, then refused when evaluated.
    fn display(&mut self, close: &str) -> Result<Node, Error> {
        while !self.eat(close) {
            self.comparison()?;
            if self.eat(":") {
                self.comparison()?;
            }
            if !self.eat(",") {
                self.expect(close)?;
                break;
            }
        }
        Ok(Node::Other)
    }

    fn slice(&mut self) -> Result<(), Error> {
        loop {
            if !matches!(self.peek_op(), Some(":" | "]")) {
                self.comparison()?;
            }
            if self.eat(":") || self.eat(",") {
                continue;
            }
            return self.expect("]");
        }
    }

    fn call(&mut self, func: Node) -> Result<Node, Error> {
        let mut args = Vec::new();
        let mut plain = true;
        while !self.eat(")") {
            if self.eat("*") || self.eat("**") {
                plain = false;
            }
            let arg = self.comparison()?;
            if self.peek_op() == Some("=") {
                return Err(syntax());
            }
            args.push(arg);
            if !self.eat(",") {
                self.expect(")")?;
                break;
            }
        }
        match func {
            Node::Name(name) if plain && FUNCTIONS.contains(&name.as_str()) => Ok(Node::Call(name, args)),
            _ => Ok(Node::Other),
        }
    }
}

fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "elif"
            | "for"
            | "while"
            | "in"
            | "is"
            | "lambda"
            | "import"
            | "from"
            | "def"
            | "class"
            | "return"
            | "yield"
            | "with"
            | "as"
            | "try"
            | "except"
            | "finally"
            | "raise"
            | "pass"
            | "del"
            | "global"
            | "nonlocal"
            | "assert"
            | "break"
            | "continue"
            | "await"
            | "async"
            | "and"
            | "or"
    )
}

/// Works a sum out. Raises the Python app's errors: "empty", "not a sum", "unknown name 'x'", "that power is
/// too big", "no finite answer" and the arithmetic ones (division by zero, a number out of range).
pub fn evaluate(text: &str, scope: &Scope) -> Result<Num, Error> {
    let source = prepare(text);
    if source.is_empty() {
        return Err(Error::Calc("empty".into()));
    }
    let mut parser = Parser { tokens: tokens(&source)?, at: 0, depth: 0 };
    let tree = parser.top()?;
    if parser.at != parser.tokens.len() {
        return Err(syntax());
    }
    let value = walk(&tree, scope)?;
    if let Num::Float(f) = value
        && !f.is_finite()
    {
        return Err(Error::Calc("no finite answer".into()));
    }
    Ok(value)
}

fn not_allowed() -> Error {
    Error::Calc("not allowed in a sum".into())
}

fn walk(node: &Node, scope: &Scope) -> Result<Num, Error> {
    match node {
        Node::Number(n) => Ok(n.clone()),
        Node::Other => Err(not_allowed()),
        Node::Name(name) => {
            if let Some(v) = scope.variables.get(name) {
                return Ok(v.clone());
            }
            match CONSTANTS.iter().find(|(n, _)| n == name) {
                Some((_, v)) => Ok(Num::Float(*v)),
                None => Err(Error::Calc(format!("unknown name '{name}'"))),
            }
        }
        Node::Negate(inner) => Ok(walk(inner, scope)?.neg()),
        Node::Plus(inner) => walk(inner, scope),
        Node::Invert(inner) => {
            let v = walk(inner, scope)?;
            if !v.is_whole() {
                return Err(Error::Calc("~ needs a whole number".into()));
            }
            Ok(Num::Int(-v.to_int()? - BigInt::one()))
        }
        Node::Binary(op, left, right) => {
            let a = walk(left, scope)?;
            let b = walk(right, scope)?;
            binary(*op, &a, &b)
        }
        Node::Call(name, args) => {
            let values = args.iter().map(|a| walk(a, scope)).collect::<Result<Vec<_>, _>>()?;
            crate::funcs::call(name, &values, scope.degrees)
        }
    }
}

fn binary(op: Binary, a: &Num, b: &Num) -> Result<Num, Error> {
    match op {
        Binary::Add => a.add(b),
        Binary::Sub => a.sub(b),
        Binary::Mul => a.mul(b),
        Binary::Div => a.div(b),
        Binary::FloorDiv => a.floor_div(b),
        Binary::Mod => a.modulo(b),
        Binary::Pow => {
            let too_big = b.abs().compare(&Num::from(i64::from(MAX_POW))).is_gt()
                || (a.abs().compare(&Num::from(1)).is_gt()
                    && b.to_f64().unwrap_or(f64::INFINITY).abs() * a.log10_abs() > 4000.0);
            if too_big {
                return Err(Error::Calc("that power is too big".into()));
            }
            a.pow(b)
        }
        Binary::And | Binary::Or | Binary::Xor | Binary::Shl | Binary::Shr => bits(op, a, b),
    }
}

fn bits(op: Binary, a: &Num, b: &Num) -> Result<Num, Error> {
    if !a.is_whole() || !b.is_whole() {
        return Err(Error::Calc("&, |, xor, << and >> need whole numbers".into()));
    }
    let (x, y) = (a.to_int()?, b.to_int()?);
    let result = match op {
        Binary::And => x & y,
        Binary::Or => x | y,
        Binary::Xor => x ^ y,
        Binary::Shl | Binary::Shr => {
            if op == Binary::Shl && y > BigInt::from(MAX_SHIFT) {
                return Err(Error::Calc("that shift is too big".into()));
            }
            if y.is_negative() {
                return Err(Error::Value("negative shift count".into()));
            }
            // a right shift by more than the number has bits leaves 0, or -1 for a negative one
            let count = y.to_usize().unwrap_or(usize::MAX);
            match op {
                Binary::Shl => x << count,
                _ if count as u64 > x.bits() => {
                    if x.is_negative() {
                        BigInt::from(-1)
                    } else {
                        BigInt::zero()
                    }
                }
                _ => x >> count,
            }
        }
        _ => unreachable!("bits is only called for bit operators"),
    };
    Ok(Num::Int(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope { variables: HashMap::new(), degrees: true }
    }

    #[test]
    fn prepares_the_way_the_python_app_does() {
        assert_eq!(prepare("54 + 10%"), "(54)*(1+10/100)");
        assert_eq!(prepare("1,000,000 + 1"), "1000000 + 1");
        assert_eq!(prepare("12,34"), "12,34");
        assert_eq!(prepare("6 xor 3"), "6 ^ 3");
        assert_eq!(prepare("2^10"), "2**10");
    }

    #[test]
    fn minus_binds_looser_than_a_power() {
        let v = evaluate("-2^2", &scope()).unwrap();
        assert_eq!(v, Num::from(-4));
        assert_eq!(evaluate("2^-1", &scope()).unwrap(), Num::Float(0.5));
    }

    #[test]
    fn deep_nesting_is_refused_not_overflowed() {
        let deep = format!("{}1{}", "(".repeat(5000), ")".repeat(5000));
        assert!(evaluate(&deep, &scope()).is_err());
    }
}
