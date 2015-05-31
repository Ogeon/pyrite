use std::iter::{Iterator, Map};
use std::fmt::Display;

macro_rules! tt_to {
    (pattern $pattern:pat) => ($pattern);

    (expression $character:expr) => ($character);
}

macro_rules! make_tokens {
    ($($name:ident => $character:tt),+ with $input:ident : $($container_name:ident => $test:expr),+ else : $other_name:ident) => (
        
        #[derive(Debug, Clone, Copy)]
        pub enum Token {
            $($name,)+
            $($container_name(char),)+
            $other_name(char)
        }

        impl Token {
            pub fn from_char(c: char) -> Token {
                match c {
                    $(tt_to!(pattern $character) => Token::$name,)+
                    $($input if $test => Token::$container_name($input),)+
                    c => Token::$other_name(c)
                }
            }

            pub fn to_char(&self) -> char {
                match *self {
                    $(Token::$name => tt_to!(expression $character),)+
                    $(Token::$container_name(c) => c,)+
                    Token::$other_name(c) => c
                }
            }
        }

        impl ::std::cmp::PartialEq for Token {
            fn eq(&self, other: &Token) -> bool {
                match (self, other) {
                    $((&Token::$name, &Token::$name) => true,)+
                    $((&Token::$container_name(a), &Token::$container_name(b)) => a == b,)+
                    (&Token::$other_name(a), &Token::$other_name(b)) => a == b,
                    _ => false
                }
            }
        }

        impl ::std::cmp::Eq for Token {}

    )
}

make_tokens! {
    Period => '.',
    Comma => ',',
    Equals => '=',
    LBrace => '{',
    RBrace => '}',
    LBracket => '[',
    RBracket => ']',
    Quote => '"',
    Underscore => '_',
    Minus => '-'

    with character:

    Alpha => character.is_alphabetic(),
    Num => character.is_numeric(),
    Whitespace => character.is_whitespace()

    else: OtherChar
}

#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    Assign(Vec<String>, Value),
    Include(String, Option<Vec<String>>)
}

#[derive(Clone, PartialEq, Debug)]
pub enum Value {
    Struct(Vec<String>, Vec<Action>),
    List(Vec<Value>),
    Str(String),
    Number(f64)
}

struct Parser<I: Iterator<Item=char>> {
    tokens: Map<I, fn(I::Item) -> Token>,
    buffer: Vec<Token>,
    line: usize,
    column: usize,
    prev_column: usize
}

impl<I: Iterator<Item=char>> Parser<I> {
    fn new(source: I) -> Parser<I> {
        Parser {
            tokens: source.map(Token::from_char),
            buffer: Vec::new(),
            line: 0,
            column: 0,
            prev_column: 0
        }
    }

    fn position(&self) -> (usize, usize) {
        (self.line, self.column)
    }

    fn previous_position(&self) -> (usize, usize) {
        if self.line == 0 {
            (0, self.prev_column)
        } else if self.column > 0 {
            (self.line, self.prev_column)
        } else {
            (self.line - 1, self.prev_column)
        }
    }

    fn next(&mut self) -> Option<Token> {
        self.buffer.pop()
        .or_else(|| self.tokens.next()).map(|t| {
            self.prev_column = self.column;

            match t {
                Token::Whitespace('\n') => {
                    self.line += 1;
                    self.column = 0;
                },
                _ => self.column += 1
            }

            t
        })
    }

    fn buffer(&mut self, token: Token) {
        self.buffer.push(token);
    }

    fn buffer_all(&mut self, tokens: Vec<Token>) {
        for token in tokens.into_iter().rev() {
            self.buffer(token);
        }
    }

    fn take_if<P: FnOnce(&Token) -> bool>(&mut self, pred: P) -> Option<Token> {
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

    fn skip_while<P: Fn(&Token) -> bool>(&mut self, pred: P) {
        loop {
            match self.next() {
                Some(ref t) if !pred(t) => {
                    self.buffer(*t);
                    break;
                },
                None => break,
                _ => {}
            }
        }
    }

    fn skip_whitespace(&mut self) {
        self.skip_while(|t| match t {
            &Token::Whitespace(_) => true,
            _ => false
        })
    }

    fn parse_ident(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();
        loop {
            match self.take_if(|t| match t {
                &Token::Alpha(_) => true,
                &Token::Num(_) => true,
                &Token::Underscore => true,
                _ => false
            }) {
                Some(t) => tokens.push(t),
                None => break
            }
        }
        tokens
    }

    fn eat_ident(&mut self, expected: &str) -> bool {
        let ident = self.parse_ident();
        let matches =
            ident.len() == expected.len() &&
            ident.iter().zip(expected.chars()).all(|(t, c)| t.to_char() == c);

        if !matches {
            self.buffer_all(ident);
        }

        matches
    }

    fn parse_path(&mut self) -> Result<Option<Vec<String>>, String> {
        let mut idents = Vec::new();
        loop {
            let pos = self.position();
            let ident = self.parse_ident();
            if ident.len() == 0 {
                if idents.len() > 0 {
                    for ident in idents.into_iter().rev() {
                        self.buffer(Token::Period);
                        self.buffer_all(ident);
                    }

                    return Err(format_error(pos, "expected an identifier"))
                } else {
                    return Ok(None)
                }
            } else {
                idents.push(ident);
                if self.take_if(|t| match t {
                    &Token::Period => true,
                    _ => false
                }).is_none() {
                    break;
                }
            }
        }

        Ok(Some(idents.into_iter().map(|ident| ident.into_iter().map(|t| t.to_char()).collect()).collect()))
    }

    fn parse_string(&mut self) -> Result<Option<String>, String> {
        let pos = self.position();
        if !self.eat(Token::Quote) {
            return Ok(None);
        }

        let mut string = String::new();

        loop {
            match self.next() {
                Some(Token::Quote) => break,
                Some(t) => string.push(t.to_char()),
                None => return Err(format_error(pos, "unmatched '\"'"))
            }
        }

        Ok(Some(string))
    }

    fn parse_number(&mut self) -> Result<Option<f64>, String> {
        let mut num_str = String::new();
        let mut integer = true;

        if self.eat(Token::Minus) {
            num_str.push('-');
        }

        loop {
            match self.next() {
                Some(Token::Num(c)) => num_str.push(c),
                Some(Token::Period) => if integer {
                    integer = false;
                    num_str.push('.');
                } else {
                    return Err(format_error(self.previous_position(), "unexpected '.'"))
                },
                Some(Token::Underscore) => {},
                Some(c) => {
                    self.buffer(c);
                    break;
                },
                None => break
            }
        }

        Ok(num_str.parse().ok())
    }
}

fn format_error<S: Display>((line, column): (usize, usize), message: S) -> String {
    format!("l{}, c{}: {}", line, column, message)
}

pub fn parse<C: Iterator<Item=char>>(source: C) -> Result<Vec<Action>, String> {
    let mut parser = Parser::new(source);
    parse_actions(&mut parser, false).map(|v| v.unwrap())
}

fn parse_actions<I: Iterator<Item=char>>(parser: &mut Parser<I>, expect_rbrace: bool) -> Result<Option<Vec<Action>>, String> {
    let mut actions = Vec::new();

    loop {
        parser.skip_while(|t| match *t {
            Token::Comma => true,
            Token::Whitespace(_) => true,
            _ => false
        });

        match try!(parse_include(parser)) {
            Some(i) => {
                actions.push(i);
                continue
            }
            _ => {}
        }

        match try!(parse_assign(parser)) {
            Some(a) => actions.push(a),
            None => if expect_rbrace && !parser.eat(Token::RBrace) {
                return Ok(None)
            } else {
                break
            }
        }
    }

    parser.skip_while(|t| match *t {
        Token::Comma => true,
        Token::Whitespace(_) => true,
        _ => false
    });

    Ok(Some(actions))
}

fn parse_assign<I: Iterator<Item=char>>(parser: &mut Parser<I>) -> Result<Option<Action>, String> {
    let path = match try!(parser.parse_path()) {
        Some(p) => p,
        None => return Ok(None)
    };

    parser.skip_whitespace();

    let pos = parser.position();

    let value = try!(match parser.next() {
        Some(Token::Equals) => {
            parser.skip_whitespace();
            parse_value(parser)
        },
        Some(t) => Err(format_error(pos, format!("expected '=', but found {}", t.to_char()))),
        None => Err(format_error(pos, "expected '='"))
    });

    Ok(Some(Action::Assign(path, value)))
}

fn parse_include<I: Iterator<Item=char>>(parser: &mut Parser<I>) -> Result<Option<Action>, String> {
    if !parser.eat_ident("include") {
        return Ok(None);
    }

    parser.skip_whitespace();

    let pos = parser.position();

    let source = match try!(parser.parse_string()) {
        Some(s) => s,
        None => return Err(format_error(pos, "expected a string"))
    };

    parser.skip_whitespace();

    if !parser.eat_ident("as") {
        return Ok(Some(Action::Include(source, None)))
    }

    parser.skip_whitespace();

    let pos = parser.position();

    let path = match try!(parser.parse_path()) {
        Some(p) => p,
        None => return Err(format_error(pos, "expected a path"))
    };

    Ok(Some(Action::Include(source, Some(path))))
}

fn parse_list<I: Iterator<Item=char>>(parser: &mut Parser<I>) -> Result<Value, String> {
    let mut elements = Vec::new();

    loop {
        parser.skip_while(|t| match *t {
            Token::Comma => true,
            Token::Whitespace(_) => true,
            _ => false
        });

        if parser.eat(Token::RBracket) {
            break;
        }

        elements.push(try!(parse_value(parser)));
    }

    parser.skip_while(|t| match *t {
        Token::Comma => true,
        Token::Whitespace(_) => true,
        _ => false
    });

    Ok(Value::List(elements))
}

fn parse_value<I: Iterator<Item=char>>(parser: &mut Parser<I>) -> Result<Value, String> {
    match try!(parser.parse_string()) {
        Some(s) => return Ok(Value::Str(s)),
        None => {}
    }

    match try!(parser.parse_number()) {
        Some(n) => return Ok(Value::Number(n)),
        None => {}
    }

    if parser.eat(Token::LBracket) {
        return parse_list(parser);
    }

    let path = try!(parser.parse_path()).unwrap_or_else(|| Vec::new());
    parser.skip_whitespace();
    let pos = parser.position();

    match parser.next() {
        Some(Token::LBrace) => match try!(parse_actions(parser, true)) {
            Some(actions) => return Ok(Value::Struct(path, actions)),
            None => return Err(format_error(pos, "unmatched '{'"))
        },
        Some(t) => {
            parser.buffer(t);
            return Ok(Value::Struct(path, Vec::new()))
        },
        None => return Ok(Value::Struct(path, Vec::new())),
    }
}

#[cfg(test)]
#[warn(dead_code)]
mod test {
    use super::Action::Assign;
    use super::Value::{
        Str,
        Number,
        List
    };

    #[test]
    fn parse_path() {
        let mut parser = super::Parser::new("path.to_somewhere".chars());
        assert_eq!(parser.parse_path(), Ok(Some(vec!["path".into(), "to_somewhere".into()])));

        let mut parser = super::Parser::new("path.to_somewhere.else".chars());
        assert_eq!(parser.parse_path(), Ok(Some(vec!["path".into(), "to_somewhere".into(), "else".into()])));
    }

    #[test]
    fn parse_assign_string() {
        let mut parser = super::Parser::new("path = \"helo\"".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec!["path".into()], Str("helo".into())))));
    }

    #[test]
    fn parse_assign_number() {
        let mut parser = super::Parser::new("path = 10".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec!["path".into()], Number(10.0)))));

        let mut parser = super::Parser::new("path = 10.123".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec!["path".into()], Number(10.123)))));

        let mut parser = super::Parser::new("path = -1_000.123".chars());
        assert_eq!(super::parse_assign(&mut parser), Ok(Some(Assign(vec!["path".into()], Number(-1_000.123)))));
    }

    #[test]
    fn parse_assign_list() {
        let mut parser = super::Parser::new("path = [\"helo\" 42]".chars());
        assert_eq!( super::parse_assign(&mut parser), Ok(Some(Assign( vec!["path".into()], List(vec![Str("helo".into()), Number(42.0)]) ))) );

        let mut parser = super::Parser::new("path = [\"helo\", 42]".chars());
        assert_eq!( super::parse_assign(&mut parser), Ok(Some(Assign( vec!["path".into()], List(vec![Str("helo".into()), Number(42.0)]) ))) );
    }
}