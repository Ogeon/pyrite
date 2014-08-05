use std::char;
use std::iter::{Iterator, Map};
use std::collections::ringbuf::RingBuf;
use std::collections::Deque;
use std::fmt::Show;
use std::num::from_str_radix;

macro_rules! tt_to(
    (pattern $pattern:pat) => ($pattern);

    (expression $character:expr) => ($character);
)

macro_rules! make_tokens(
    ($($name:ident => $character:tt),+ with $input:ident : $($container_name:ident => $test:expr),+ else : $other_name:ident) => (
        
        #[deriving(Show)]
        pub enum Token {
            $($name,)+
            $($container_name(char),)+
            $other_name(char)
        }

        impl Token {
            pub fn from_char(c: char) -> Token {
                match c {
                    $(tt_to!(pattern $character) => $name,)+
                    $($input if $test => $container_name($input),)+
                    c => $other_name(c)
                }
            }

            pub fn to_char(&self) -> char {
                match *self {
                    $($name => tt_to!(expression $character),)+
                    $($container_name(c) => c,)+
                    $other_name(c) => c
                }
            }
        }

        impl ::std::cmp::PartialEq for Token {
            fn eq(&self, other: &Token) -> bool {
                match (self, other) {
                    $((&$name, &$name) => true,)+
                    $((&$container_name(a), &$container_name(b)) => a == b,)+
                    (&$other_name(a), &$other_name(b)) => a == b,
                    _ => false
                }
            }
        }

        impl ::std::cmp::Eq for Token {}

    )
)

make_tokens! {
    Period => '.',
    Comma => ',',
    Equals => '=',
    LBrace => '{',
    RBrace => '}',
    Quote => '"',
    Underscore => '_',
    Minus => '-'

    with character:

    Alpha => char::is_alphabetic(character),
    Num => char::is_digit(character),
    Whitespace => char::is_whitespace(character)

    else: OtherChar
}

#[deriving(Clone, PartialEq, Show)]
pub enum Action {
    Assign(Vec<String>, Value)
}

#[deriving(Clone, PartialEq, Show)]
pub enum Value {
    Struct(Vec<String>, Vec<Action>),
    String(String),
    Number(f64)
}

struct Parser<'a, I> {
    tokens: Map<'a, char, Token, I>,
    buffer: RingBuf<Token>,
    line: uint,
    column: uint,
    prev_column: uint
}

impl<'a, I: Iterator<char>> Parser<'a, I> {
    fn new<'a>(source: I) -> Parser<'a, I> {
        Parser {
            tokens: source.map(|c| Token::from_char(c)),
            buffer: RingBuf::new(),
            line: 0,
            column: 0,
            prev_column: 0
        }
    }

    fn position(&self) -> (uint, uint) {
        (self.line, self.column)
    }

    fn previous_position(&self) -> (uint, uint) {
        if self.line == 0 {
            (0, self.prev_column)
        } else if self.column > 0 {
            (self.line, self.prev_column)
        } else {
            (self.line - 1, self.prev_column)
        }
    }

    fn next(&mut self) -> Option<Token> {
        self.buffer.pop_front()
        .or_else(|| self.tokens.next()).map(|t| {
            self.prev_column = self.column;

            match t {
                Whitespace('\n') => {
                    self.line += 1;
                    self.column = 0;
                },
                _ => self.column += 1
            }

            t
        })
    }

    fn buffer(&mut self, token: Token) {
        self.buffer.push_front(token);
    }

    fn buffer_all(&mut self, tokens: Vec<Token>) {
        for token in tokens.move_iter().rev() {
            self.buffer(token);
        }
    }

    fn take_if(&mut self, pred: |&Token| -> bool) -> Option<Token> {
        self.next().and_then(|t|
            if pred(&t) {
                Some(t)
            } else {
                self.buffer(t);
                None
            }
        )
    }

    fn eat(&mut self, token: Token) -> bool {
        self.take_if(|&t| t == token).is_some()
    }

    fn skip_while(&mut self, pred: |&Token| -> bool) {
        loop {
            match self.next() {
                Some(t) if !pred(&t) => {
                    self.buffer(t);
                    break;
                },
                None => break,
                _ => {}
            }
        }
    }

    fn skip_whitespace(&mut self) {
        self.skip_while(|t| match t {
            &Whitespace(_) => true,
            _ => false
        })
    }

    fn parse_ident(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            match self.take_if(|t| match t {
                &Alpha(_) => true,
                &Num(_) => true,
                &Underscore => true,
                _ => false
            }) {
                Some(t) => tokens.push(t),
                None => break
            }
        }
        tokens
    }

    fn parse_path(&mut self) -> Result<Option<Vec<String>>, String> {
        let mut idents = Vec::new();
        loop {
            let pos = self.position();
            let ident = self.parse_ident();
            if ident.len() == 0 {
                if idents.len() > 0 {
                    for ident in idents.move_iter().rev() {
                        self.buffer(Period);
                        self.buffer_all(ident);
                    }

                    return Err(format_error(pos, "expected an identifier"))
                } else {
                    return Ok(None)
                }
            } else {
                idents.push(ident);
                if self.take_if(|t| match t {
                    &Period => true,
                    _ => false
                }).is_none() {
                    break;
                }
            }
        }

        Ok(Some(idents.move_iter().map(|ident| ident.move_iter().map(|t| t.to_char()).collect()).collect()))
    }

    fn parse_string(&mut self) -> Result<Option<String>, String> {
        let pos = self.position();
        if !self.eat(Quote) {
            return Ok(None);
        }

        let mut string = String::new();

        loop {
            match self.next() {
                Some(Quote) => break,
                Some(t) => string.push_char(t.to_char()),
                None => return Err(format_error(pos, "unmatched '\"'"))
            }
        }

        Ok(Some(string))
    }

    fn parse_number(&mut self) -> Result<Option<f64>, String> {
        let mut num_str = String::new();
        let mut integer = true;

        if self.eat(Minus) {
            num_str.push_char('-');
        }

        loop {
            match self.next() {
                Some(Num(c)) => num_str.push_char(c),
                Some(Period) => if integer {
                    integer = false;
                    num_str.push_char('.');
                } else {
                    return Err(format_error(self.previous_position(), "unexpected '.'"))
                },
                Some(Underscore) => {},
                Some(c) => {
                    self.buffer(c);
                    break;
                },
                None => break
            }
        }

        Ok(from_str_radix(num_str.as_slice(), 10))
    }
}

fn format_error<S: Show>((line, column): (uint, uint), message: S) -> String {
    format!("l{}, c{}: {}", line, column, message)
}

pub fn parse<C: Iterator<char>>(source: C) -> Result<Vec<Action>, String> {
    let mut parser = Parser::new(source);
    parse_assignments(&mut parser, false).map(|v| v.unwrap())
}

fn parse_assignments<I: Iterator<char>>(parser: &mut Parser<I>, expect_rbrace: bool) -> Result<Option<Vec<Action>>, String> {
    let mut assignments = Vec::new();

    loop {
        parser.skip_whitespace();

        match try!(parse_assign(parser)) {
            Some(a) => assignments.push(a),
            None => if expect_rbrace && !parser.eat(RBrace) {
                return Ok(None)
            } else {
                break
            }
        }
    }

    Ok(Some(assignments))
}

fn parse_assign<I: Iterator<char>>(parser: &mut Parser<I>) -> Result<Option<Action>, String> {
    let path = match try!(parser.parse_path()) {
        Some(p) => p,
        None => return Ok(None)
    };

    parser.skip_whitespace();

    let pos = parser.position();

    let value = try!(match parser.next() {
        Some(Equals) => {
            parser.skip_whitespace();
            parse_value(parser)
        },
        Some(t) => Err(format_error(pos, format!("expected '=', but found {}", t.to_char()))),
        None => Err(format_error(pos, "expected '='"))
    });

    Ok(Some(Assign(path, value)))
}

fn parse_value<I: Iterator<char>>(parser: &mut Parser<I>) -> Result<Value, String> {
    match try!(parser.parse_string()) {
        Some(s) => return Ok(String(s)),
        None => {}
    }

    match try!(parser.parse_number()) {
        Some(n) => return Ok(Number(n)),
        None => {}
    }

    let path = try!(parser.parse_path()).unwrap_or_else(|| Vec::new());
    parser.skip_whitespace();
    let pos = parser.position();

    match parser.next() {
        Some(LBrace) => match try!(parse_assignments(parser, true)) {
            Some(assignments) => return Ok(Struct(path, assignments)),
            None => return Err(format_error(pos, "unmatched '{'"))
        },
        Some(t) => {
            parser.buffer(t);
            return Ok(Struct(path, Vec::new()))
        },
        None => return Ok(Struct(path, Vec::new())),
    }
}

#[cfg(test)]
mod test {
    use super::{
        Assign,
        String,
        Number
    };

    #[test]
    fn parse_path() {
        let mut parser = super::Parser::new("path.to_somewhere".chars());
        assert_eq!(parser.parse_path(), Ok(Some(vec![String::from_str("path"), String::from_str("to_somewhere")])));

        let mut parser = super::Parser::new("path.to_somewhere.else".chars());
        assert_eq!(parser.parse_path(), Ok(Some(vec![String::from_str("path"), String::from_str("to_somewhere"), String::from_str("else")])));
    }

    #[test]
    fn parse_assign_string() {
        let mut parser = super::Parser::new("path = \"helo\"".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec![String::from_str("path")], String(String::from_str("helo"))))));
    }

    #[test]
    fn parse_assign_number() {
        let mut parser = super::Parser::new("path = 10".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec![String::from_str("path")], Number(10.0)))));

        let mut parser = super::Parser::new("path = 10.123".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec![String::from_str("path")], Number(10.123)))));

        let mut parser = super::Parser::new("path = -1_000.123".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec![String::from_str("path")], Number(-1_000.123)))));
    }
}