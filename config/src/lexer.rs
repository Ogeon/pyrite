use std::ops::Deref;

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Position {
    pub line: usize,
    pub column: usize
}

impl Position {
    fn next_line(&self) -> Position {
        Position {
            line: self.line + 1,
            column: 0
        }
    }

    fn next_column(&self) -> Position {
        Position {
            line: self.line,
            column: self.column + 1
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Bracket {
    ///( )
    Parenthesis,
    ///{ }
    Brace,
    ///[ ]
    Square
}

impl From<char> for Bracket {
    fn from(c: char) -> Bracket {
        match c {
            '(' | ')' => Bracket::Parenthesis,
            '{' | '}' => Bracket::Brace,
            '[' | ']' => Bracket::Square,

            //Not the best, but it shouldn't be triggered! Just, don't!
            _ => unreachable!()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CharacterType {
    Open(Bracket),
    Close(Bracket),
    Equals,
    Comma,
    Dot,
    Quote,
    BackSlash,
    Whitespace(char),
    Numeric(u8),
    Other(char)
}

impl CharacterType {
    pub fn is_whitespace(&self) -> bool {
        if let CharacterType::Whitespace(_) = *self {
            true
        } else {
            false
        }
    }
}

impl From<char> for CharacterType {
    fn from(c: char) -> CharacterType {
        match c {
            '(' | '{' | '[' => CharacterType::Open(c.into()),
            ')' | '}' | ']' => CharacterType::Close(c.into()),
            '=' => CharacterType::Equals,
            ',' => CharacterType::Comma,
            '.' => CharacterType::Dot,
            '"' => CharacterType::Quote,
            '\\' => CharacterType::BackSlash,
            c if c.is_whitespace() => CharacterType::Whitespace(c),
            c => if let Some(d) = c.to_digit(10) {
                CharacterType::Numeric(d as u8)
            } else {
                CharacterType::Other(c)
            }
        }
    }
}

impl From<CharacterType> for char {
    fn from(c: CharacterType) -> char {
        match c {
            CharacterType::Open(Bracket::Parenthesis) => '(',
            CharacterType::Open(Bracket::Brace) => '{',
            CharacterType::Open(Bracket::Square) => '[',

            CharacterType::Close(Bracket::Parenthesis) => ')',
            CharacterType::Close(Bracket::Brace) => '}',
            CharacterType::Close(Bracket::Square) => ']',

            CharacterType::Equals => '=',
            CharacterType::Comma => ',',
            CharacterType::Dot => '.',
            CharacterType::Quote => '"',
            CharacterType::BackSlash => '\\',
            CharacterType::Whitespace(c) | CharacterType::Other(c) => c,
            CharacterType::Numeric(n) => ::std::char::from_digit(n as u32, 10).expect("non base 10 digit stored as digit")
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Character {
    pub ty: CharacterType,
    pub position: Position
}

impl From<Character> for char {
    fn from(c: Character) -> char {
        c.ty.into()
    }
}

pub struct Lexer<I> {
    source: I,
    position: Position,
    buffer: Option<Character>
}

impl<I: Iterator<Item=char>> Lexer<I> {
   pub  fn new(source: I) -> Lexer<I> {
        Lexer {
            source: source,
            position: Position {
                line: 0,
                column: 0
            },
            buffer: None
        }
    }

    pub fn skip_whitespace(&mut self) {
        while let Some(c) = self.next() {
            if !c.ty.is_whitespace() {
                self.buffer = Some(c);
                break;
            }
        }
    }

    pub fn peek<'a>(&'a mut self) -> Option<Peek<'a, I>> {
        self.next().map(move |c| Peek {
            character: Some(c),
            lexer: self
        })
    }

    pub fn position(&self) -> Position {
        self.position
    }
}

impl<I: Iterator<Item=char>> Iterator for Lexer<I> {
    type Item = Character;

    fn next(&mut self) -> Option<Character> {
        let buffered = self.buffer.take();
        if buffered.is_some() {
            return buffered;
        }

        if let Some(c) = self.source.next().map(|c| c.into()) {
            let res = Character {
                ty: c,
                position: self.position
            };

            if let CharacterType::Whitespace('\n') = c {
                self.position = self.position.next_line();
            } else {
                self.position = self.position.next_column();
            }

            Some(res)
        } else {
            None
        }
    }
}

pub struct Peek<'a, I: 'a> {
    character: Option<Character>,
    lexer: &'a mut Lexer<I>,
}

impl<'a, I: 'a> Peek<'a, I> {
    pub fn take(mut self) -> Character {
        self.character.take().expect("use of Peek after drop")
    }
}

impl<'a, I: 'a> Deref for Peek<'a, I> {
    type Target = Character;

    fn deref(&self) -> &Character {
        self.character.as_ref().expect("use of Peek after drop")
    }
}

impl<'a, I: 'a> Drop for Peek<'a, I> {
    fn drop(&mut self) {
        self.lexer.buffer = self.character.take();
    }
}

impl<'a, I: 'a> From<Peek<'a, I>> for char {
    fn from(p: Peek<'a, I>) -> char {
        p.ty.into()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Lexer,
        Character,
        CharacterType,
        Bracket,
        Position
    };

    #[test]
    fn character_types() {
        let mut lexer = Lexer::new("{[()]}=,.\"\\ \n8$".chars());
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Open(Bracket::Brace),
            position: Position {
                line: 0,
                column: 0
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Open(Bracket::Square),
            position: Position {
                line: 0,
                column: 1
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Open(Bracket::Parenthesis),
            position: Position {
                line: 0,
                column: 2
            }
        }));

        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Close(Bracket::Parenthesis),
            position: Position {
                line: 0,
                column: 3
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Close(Bracket::Square),
            position: Position {
                line: 0,
                column: 4
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Close(Bracket::Brace),
            position: Position {
                line: 0,
                column: 5
            }
        }));

        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Equals,
            position: Position {
                line: 0,
                column: 6
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Comma,
            position: Position {
                line: 0,
                column: 7
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Dot,
            position: Position {
                line: 0,
                column: 8
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Quote,
            position: Position {
                line: 0,
                column: 9
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::BackSlash,
            position: Position {
                line: 0,
                column: 10
            }
        }));

        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Whitespace(' '),
            position: Position {
                line: 0,
                column: 11
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Whitespace('\n'),
            position: Position {
                line: 0,
                column: 12
            }
        }));

        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Numeric(8),
            position: Position {
                line: 1,
                column: 0
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Other('$'),
            position: Position {
                line: 1,
                column: 1
            }
        }));

        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn skip_whitespace() {
        let mut lexer = Lexer::new("a     \\   \n  b".chars());
        lexer.skip_whitespace();
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Other('a'),
            position: Position {
                line: 0,
                column: 0
            }
        }));
        lexer.skip_whitespace();
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::BackSlash,
            position: Position {
                line: 0,
                column: 6
            }
        }));
        lexer.skip_whitespace();
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Other('b'),
            position: Position {
                line: 1,
                column: 2
            }
        }));
        assert_eq!(lexer.next(), None);
    }

    #[test]
    fn peek() {
        let mut lexer = Lexer::new("abc".chars());
        assert_eq!(lexer.peek().as_ref().map(|p| &**p), Some(&Character {
            ty: CharacterType::Other('a'),
            position: Position {
                line: 0,
                column: 0
            }
        }));
        assert_eq!(lexer.peek().map(|p| p.take()), Some(Character {
            ty: CharacterType::Other('a'),
            position: Position {
                line: 0,
                column: 0
            }
        }));
        assert_eq!(lexer.peek().as_ref().map(|p| &**p), Some(&Character {
            ty: CharacterType::Other('b'),
            position: Position {
                line: 0,
                column: 1
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Other('b'),
            position: Position {
                line: 0,
                column: 1
            }
        }));
        assert_eq!(lexer.next(), Some(Character {
            ty: CharacterType::Other('c'),
            position: Position {
                line: 0,
                column: 2
            }
        }));
        assert_eq!(lexer.next(), None);
    }
}
