use lexer::{Lexer, Character, CharacterType, Bracket, Position};

const KW_INCLUDE: &'static str = "include";
const KW_ROOT: &'static str = "root";

macro_rules! expect_or_eof {
    ($e: expr) => (
        if let Some(v) = $e {
            v
        } else {
            return Err(None)
        }
    )
}

pub fn parse<I: IntoIterator<Item=char>>(source: I) -> Result<Vec<Span<Statement>>, Error> {
    let mut parser = Parser::new(source.into_iter());
    let mut statements = vec![];

    loop {
        match parser.parse_statement() {
            Ok(statement) => statements.push(statement),
            Err(Some(e)) => return Err(e),
            _ => break
        }
    }

    Ok(statements)
}

#[derive(Debug)]
pub enum Error {
    UnexpectedCharacter(Character),
    UnexpectedKeyword(Span<String>),
    UnexpectedInclude(Position, Position),
    ExpectedValue(Position)
}

#[derive(PartialEq, Debug)]
pub struct Span<T> {
    pub item: T,
    pub from: Position,
    pub to: Position
}

#[derive(PartialEq, Debug)]
pub enum Statement {
    Include(String),
    Assign(Path, Value)
}

#[derive(PartialEq, Debug)]
pub enum PathType {
    Global,
    Local
}

#[derive(PartialEq, Debug)]
pub struct Path {
    pub path_type: PathType,
    pub path: Vec<String>
}

#[derive(PartialEq, Debug)]
pub enum Value {
    Object(Object),
    Number(Number),
    String(String),
    List(Vec<Value>)
}

#[derive(PartialEq, Debug)]
pub enum Object {
    New(Vec<(Path, Value)>),
    Extension(Path, Option<ExtensionChanges>)
}

#[derive(PartialEq, Debug)]
pub enum ExtensionChanges {
    BlockStyle(Vec<(Path, Value)>),
    FunctionStyle(Vec<Value>)
}

#[derive(PartialEq, Debug)]
pub enum Number {
    Integer(i64),
    Float(f64)
}

struct Parser<I> {
    lexer: Lexer<I>
}

impl<I: Iterator<Item=char>> Parser<I> {
    fn new(source: I) -> Parser<I> {
        Parser {
            lexer: Lexer::new(source)
        }
    }

    fn parse_statement(&mut self) -> Result<Span<Statement>, Option<Error>> {
        let Span {
            item: path,
            from: first_pos,
            to: last_pos
        } = try!(self.parse_path());

        self.lexer.skip_whitespace();

        if path.path_type == PathType::Local && path.path.len() == 1 {
            if let Some(ident) = path.path.first() {
                if ident == KW_INCLUDE {
                    let Span {
                        item: filename,
                        to: last_pos,
                        ..
                    } = try!(self.parse_string());

                    return Ok(Span {
                        item: Statement::Include(filename),
                        from: first_pos,
                        to: last_pos
                    });
                }
            }
        }

        if let Some(keyword) = find_keywords(&path.path) {
            return Err(Some(Error::UnexpectedKeyword(Span {
                item: keyword,
                from: first_pos,
                to: last_pos
            })))
        }

        let c = expect_or_eof!(self.lexer.next());

        if let CharacterType::Equals = c.ty {
            self.lexer.skip_whitespace();

            let Span {
                item: value,
                to: last_pos,
                ..
            } = try!(self.parse_value());

            Ok(Span {
                item: Statement::Assign(path, value),
                from: first_pos,
                to: last_pos
            })
        } else {
            Err(Some(Error::UnexpectedCharacter(c)))
        }
    }

    fn parse_path(&mut self) -> Result<Span<Path>, Option<Error>> {
        let mut path = vec![];
        let mut path_type = PathType::Local;
        let mut first_pos = None;
        let mut last_pos;

        loop {
            let ident = try!(self.parse_ident());
            if ident.item == KW_ROOT && path_type == PathType::Local && path.len() == 0 {
                let Span {
                    from,
                    to,
                    ..
                } = ident;
                
                if first_pos.is_none() {
                    first_pos = Some(from);
                }

                last_pos = to;

                path_type = PathType::Global;
            } else if ident.item == KW_ROOT {
                return Err(Some(Error::UnexpectedKeyword(ident)))
            } else {
                let Span {
                    item: ident,
                    from,
                    to,
                } = ident;

                if first_pos.is_none() {
                    first_pos = Some(from);
                }

                last_pos = to;

                path.push(ident);
            }

            if let Some(c) = self.lexer.peek() {
                if let CharacterType::Dot = c.ty {
                    c.take();
                    continue;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok(Span {
            item: Path {
                path_type: path_type,
                path: path
            },
            from: first_pos.unwrap_or(last_pos),
            to: last_pos
        })
    }

    fn parse_ident(&mut self) -> Result<Span<String>, Option<Error>> {
        let mut ident = String::new();
        let first_pos = {
            let first = expect_or_eof!(self.lexer.peek());
            if let CharacterType::Other(_) = first.ty {
                let first_pos = first.position;
                ident.push(first.take().into());
                first_pos
            } else {
                return Err(Some(Error::UnexpectedCharacter(first.clone())))
            }
        };
        let mut last_pos = first_pos;

        while let Some(c) = self.lexer.peek() {
            match c.ty {
                CharacterType::Numeric(_) | CharacterType::Other(_) => {
                    last_pos = c.position;
                    ident.push(c.take().into());
                },
                _ => break
            }
        }

        Ok(Span {
            item: ident,
            from: first_pos,
            to: last_pos
        })
    }

    fn parse_string(&mut self) -> Result<Span<String>, Option<Error>> {
        let first_pos = {
            let first = expect_or_eof!(self.lexer.peek());
            if let CharacterType::Quote = first.ty {
                first.take().position
            } else {
                return Err(Some(Error::UnexpectedCharacter(first.clone())))
            }
        };
        let mut string = String::new();
        let mut last_pos = first_pos;

        while let Some(c) = self.lexer.next() {
            match c.ty {
                CharacterType::Quote => {
                    last_pos = c.position;
                    break;
                },
                CharacterType::BackSlash => {
                    if let Some(c) = self.lexer.next() {
                        last_pos = c.position;
                        string.push(c.into());
                    }
                },
                ty => {
                    last_pos = c.position;
                    string.push(ty.into());
                }
            }
        }

        Ok(Span {
            item: string,
            from: first_pos,
            to: last_pos
        })
    }

    fn parse_number(&mut self) -> Result<Span<Number>, Option<Error>> {
        let mut whole_part = 0;
        let mut negative = false;
        let first_pos = {
            let first_pos = {
                let first = expect_or_eof!(self.lexer.peek());
                if let CharacterType::Other('-') = first.ty {
                    negative = true;
                } else if let CharacterType::Numeric(n) = first.ty {
                    whole_part = n as i64;
                } else {
                    return Err(Some(Error::UnexpectedCharacter(first.clone())))
                }
                first.take().position
            };

            if negative {
                let second = expect_or_eof!(self.lexer.peek());
                if let CharacterType::Numeric(n) = second.ty {
                    second.take();
                    whole_part = -(n as i64);
                } else {
                    return Err(Some(Error::UnexpectedCharacter(second.clone())))
                }
            }

            first_pos
        };
        let mut last_pos = first_pos;

        while let Some(c) = self.lexer.peek() {
            match c.ty {
                CharacterType::Numeric(n) => {
                    last_pos = c.take().position;
                    whole_part *= 10;
                    if negative {
                        whole_part -= n as i64;
                    } else {
                        whole_part += n as i64;
                    }
                },
                _ => break
            }
        }

        let float = if let Some(c) = self.lexer.peek() {
            if let CharacterType::Dot = c.ty {
                last_pos = c.take().position;
                true
            } else {
                false
            }
        } else {
            false
        };

        if !float {
            return Ok(Span {
                item: Number::Integer(whole_part),
                from: first_pos,
                to: last_pos
            });
        }

        let mut fraction_part = 0;
        let mut divider = 1u64;

        while let Some(c) = self.lexer.peek() {
            match c.ty {
                CharacterType::Numeric(n) => {
                    last_pos = c.take().position;
                    fraction_part *= 10;
                    fraction_part += n as u64;
                    divider *= 10;
                },
                _ => break
            }
        }

        let res = if negative {
            whole_part as f64 - (fraction_part as f64 / divider as f64)
        } else {
            whole_part as f64 + (fraction_part as f64 / divider as f64)
        };

        Ok(Span {
            item: Number::Float(res),
            from: first_pos,
            to: last_pos
        })
    }

    fn parse_list(&mut self) -> Result<Span<Vec<Value>>, Option<Error>> {
        self.parse_sequence(Bracket::Square)
    }

    fn parse_sequence(&mut self, bracket: Bracket) -> Result<Span<Vec<Value>>, Option<Error>> {
        let first_pos = {
            let first = expect_or_eof!(self.lexer.peek());
            if CharacterType::Open(bracket) == first.ty {
                first.take().position
            } else {
                return Err(Some(Error::UnexpectedCharacter(first.clone())))
            }
        };
        let mut items = vec![];
        let mut last_pos;

        self.lexer.skip_whitespace();

        if let Some(c) = self.lexer.peek() {
            if CharacterType::Close(bracket) == c.ty {
                return Ok(Span {
                    item: items,
                    from: first_pos,
                    to: c.take().position
                })
            }
        } else {
            return Err(None)
        }

        loop {
            let Span {
                item,
                ..
            } = try!(self.parse_value());
            items.push(item);
            self.lexer.skip_whitespace();

            let c = expect_or_eof!(self.lexer.next());
            match c.ty {
                CharacterType::Close(b) if b == bracket => {
                    last_pos = c.position;
                    break;
                },
                CharacterType::Comma => {
                    self.lexer.skip_whitespace();
                },
                _ => {
                    return Err(Some(Error::UnexpectedCharacter(c)))
                }
            }
        }

        Ok(Span {
            item: items,
            from: first_pos,
            to: last_pos
        })
    }

    fn parse_object(&mut self) -> Result<Span<Object>, Option<Error>> {
        if let Ok(path) = self.parse_path() {
            let Span {
                item: path,
                from: first_pos,
                to: last_pos
            } = path;

            if let Some(keyword) = find_keywords(&path.path) {
                return Err(Some(Error::UnexpectedKeyword(Span {
                    item: keyword,
                    from: first_pos,
                    to: last_pos
                })))
            }

            self.lexer.skip_whitespace();

            if let Some(extension) = self.parse_sequence(Bracket::Parenthesis).map(|items| Span {
                item: ExtensionChanges::FunctionStyle(items.item),
                from: items.from,
                to: items.to
            }).or_else(|_| self.parse_block().map(|items| Span {
                item: ExtensionChanges::BlockStyle(items.item),
                from: items.from,
                to: items.to
            })).ok() {
                let Span {
                    item: extension,
                    to: last_pos,
                    ..
                } = extension;

                Ok(Span {
                    item: Object::Extension(path, Some(extension)),
                    from: first_pos,
                    to: last_pos
                })
            } else {
                Ok(Span {
                    item: Object::Extension(path, None),
                    from: first_pos,
                    to: last_pos
                })
            }
        } else {
            let Span {
                item,
                from,
                to
            } = try!(self.parse_block());

            Ok(Span {
                item: Object::New(item),
                from: from,
                to: to
            })
        }
    }

    fn parse_block(&mut self) -> Result<Span<Vec<(Path, Value)>>, Option<Error>> {
        let first_pos = {
            let first = expect_or_eof!(self.lexer.peek());
            if let CharacterType::Open(Bracket::Brace) = first.ty {
                first.take().position
            } else {
                return Err(Some(Error::UnexpectedCharacter(first.clone())))
            }
        };
        let mut items = vec![];
        let mut last_pos;

        self.lexer.skip_whitespace();

        if let Some(c) = self.lexer.peek() {
            if let CharacterType::Close(Bracket::Brace) = c.ty {
                return Ok(Span {
                    item: items,
                    from: first_pos,
                    to: c.take().position
                })
            }
        } else {
            return Err(None)
        }

        loop {
            let Span {
                item,
                from,
                to
            } = try!(self.parse_statement());

            match item {
                Statement::Include(_) => return Err(Some(Error::UnexpectedInclude(from, to))),
                Statement::Assign(path, value) => items.push((path, value))
            }

            self.lexer.skip_whitespace();

            let c = expect_or_eof!(self.lexer.peek());
            if let CharacterType::Close(Bracket::Brace) = c.ty {
                last_pos = c.take().position;
                break;
            }
        }

        Ok(Span {
            item: items,
            from: first_pos,
            to: last_pos
        })
    }

    fn parse_value(&mut self) -> Result<Span<Value>, Option<Error>> {
        let pos = self.lexer.position();

        if let Ok(string) = self.parse_string() {
            Ok(Span {
                item: Value::String(string.item),
                from: string.from,
                to: string.to
            })
        } else if let Ok(num) = self.parse_number() {
            Ok(Span {
                item: Value::Number(num.item),
                from: num.from,
                to: num.to
            })
        } else if let Ok(list) = self.parse_list() {
            Ok(Span {
                item: Value::List(list.item),
                from: list.from,
                to: list.to
            })
        } else if let Ok(object) = self.parse_object() {
            Ok(Span {
                item: Value::Object(object.item),
                from: object.from,
                to: object.to
            })
        } else {
            Err(Some(Error::ExpectedValue(pos)))
        }
    }
}

fn is_keyword(word: &str) -> bool {
    if word == KW_ROOT || word == KW_INCLUDE {
        true
    } else {
        false
    }
}

fn find_keywords(words: &[String]) -> Option<String> {
    for word in words {
        if is_keyword(word) {
            return Some(word.clone())
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use parser::{
        Parser,
        Span,
        Path,
        PathType,
        Number,
        Object,
        ExtensionChanges,
        Value,
        Statement
    };

    use lexer::Position;

    #[test]
    fn ident() {
        let mut parser = Parser::new("hello".chars());
        assert_eq!(parser.parse_ident().unwrap(), Span {
            item: "hello".into(),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 4}
        });

        let mut parser = Parser::new("hello world".chars());
        assert_eq!(parser.parse_ident().unwrap(), Span {
            item: "hello".into(),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 4}
        });
        parser.lexer.next();
        assert_eq!(parser.parse_ident().unwrap(), Span {
            item: "world".into(),
            from: Position { line: 0, column: 6},
            to: Position { line: 0, column: 10}
        });
    }

    #[test]
    fn path() {
        let mut parser = Parser::new("hello".chars());
        assert_eq!(parser.parse_path().unwrap(), Span {
            item: Path {
                path_type: PathType::Local,
                path: vec!["hello".into()]
            },
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 4}
        });

        let mut parser = Parser::new("hello.world".chars());
        assert_eq!(parser.parse_path().unwrap(), Span {
            item: Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            },
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 10}
        });

        let mut parser = Parser::new("root.hello.world".chars());
        assert_eq!(parser.parse_path().unwrap(), Span {
            item: Path {
                path_type: PathType::Global,
                path: vec!["hello".into(), "world".into()]
            },
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 15}
        });
        
        let mut parser = Parser::new("hello.world.".chars());
        assert!(parser.parse_path().is_err());
        let mut parser = Parser::new("hello..world".chars());
        assert!(parser.parse_path().is_err());
        let mut parser = Parser::new("root.root.hello.world".chars());
        assert!(parser.parse_path().is_err());
        let mut parser = Parser::new("hello.root.world".chars());
        assert!(parser.parse_path().is_err());
    }

    #[test]
    fn string() {
        let mut parser = Parser::new("\"hello\"".chars());
        assert_eq!(parser.parse_string().unwrap(), Span {
            item: "hello".into(),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 6}
        });

        let mut parser = Parser::new("\"hel\\\"lo\"".chars());
        assert_eq!(parser.parse_string().unwrap(), Span {
            item: "hel\"lo".into(),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 8}
        });

        let mut parser = Parser::new("\"hel\\\\lo\"".chars());
        assert_eq!(parser.parse_string().unwrap(), Span {
            item: "hel\\lo".into(),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 8}
        });
        
        let mut parser = Parser::new("hello".chars());
        assert!(parser.parse_string().is_err());
    }

    #[test]
    fn number() {
        let mut parser = Parser::new("12".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Integer(12),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 1}
        });

        let mut parser = Parser::new("-12".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Integer(-12),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 2}
        });

        let mut parser = Parser::new("-1202".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Integer(-1202),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 4}
        });

        let mut parser = Parser::new("12.0".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Float(12.0),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 3}
        });

        let mut parser = Parser::new("-12.0".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Float(-12.0),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 4}
        });

        let mut parser = Parser::new("12.123456".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Float(12.123456),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 8}
        });

        let mut parser = Parser::new("12.".chars());
        assert_eq!(parser.parse_number().unwrap(), Span {
            item: Number::Float(12.0),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 2}
        });
        
        let mut parser = Parser::new("-".chars());
        assert!(parser.parse_number().is_err());
        let mut parser = Parser::new("-.".chars());
        assert!(parser.parse_number().is_err());
    }

    #[test]
    fn list() {
        let mut parser = Parser::new("[]".chars());
        assert_eq!(parser.parse_list().unwrap(), Span {
            item: vec![],
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 1}
        });

        let mut parser = Parser::new("[12.0]".chars());
        assert_eq!(parser.parse_list().unwrap(), Span {
            item: vec![Value::Number(Number::Float(12.0))],
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 5}
        });

        let mut parser = Parser::new("[ 12.0 ]".chars());
        assert_eq!(parser.parse_list().unwrap(), Span {
            item: vec![Value::Number(Number::Float(12.0))],
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 7}
        });

        let mut parser = Parser::new("[ 12.0, \"hello\" ]".chars());
        assert_eq!(parser.parse_list().unwrap(), Span {
            item: vec![Value::Number(Number::Float(12.0)), Value::String("hello".into())],
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 16}
        });

        let mut parser = Parser::new("[ -12.0 , \"hello\" ]".chars());
        assert_eq!(parser.parse_list().unwrap(), Span {
            item: vec![Value::Number(Number::Float(-12.0)), Value::String("hello".into())],
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 18}
        });
        
        let mut parser = Parser::new("[,]".chars());
        assert!(parser.parse_list().is_err());
        
        let mut parser = Parser::new("[1,]".chars());
        assert!(parser.parse_list().is_err());
    }

    #[test]
    fn object() {
        let mut parser = Parser::new("hello.world".chars());
        assert_eq!(parser.parse_object().unwrap(), Span {
            item: Object::Extension(Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            }, None),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 10}
        });
        let mut parser = Parser::new("hello.world ( 12 )".chars());
        assert_eq!(parser.parse_object().unwrap(), Span {
            item: Object::Extension(Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            }, Some(ExtensionChanges::FunctionStyle(vec![
                Value::Number(Number::Integer(12))
            ]))),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 17}
        });
        let mut parser = Parser::new("hello.world { hello = 12 }".chars());
        assert_eq!(parser.parse_object().unwrap(), Span {
            item: Object::Extension(Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            }, Some(ExtensionChanges::BlockStyle(vec![
                (Path {
                    path_type: PathType::Local,
                    path: vec!["hello".into()]
                }, Value::Number(Number::Integer(12)))
            ]))),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 25}
        });
        let mut parser = Parser::new("{ hello = 12 }".chars());
        assert_eq!(parser.parse_object().unwrap(), Span {
            item: Object::New(vec![
                (Path {
                    path_type: PathType::Local,
                    path: vec!["hello".into()]
                }, Value::Number(Number::Integer(12)))
            ]),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 13}
        });
    }

    #[test]
    fn value() {
        let mut parser = Parser::new("\"hello\"".chars());
        assert_eq!(parser.parse_value().unwrap(), Span {
            item: Value::String("hello".into()),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 6}
        });

        let mut parser = Parser::new("-12.0".chars());
        assert_eq!(parser.parse_value().unwrap(), Span {
            item: Value::Number(Number::Float(-12.0)),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 4}
        });

        let mut parser = Parser::new("[-12.0]".chars());
        assert_eq!(parser.parse_value().unwrap(), Span {
            item: Value::List(vec![Value::Number(Number::Float(-12.0))]),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 6}
        });
        let mut parser = Parser::new("hello.world".chars());
        assert_eq!(parser.parse_value().unwrap(), Span {
            item: Value::Object(Object::Extension(Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            }, None)),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 10}
        });
    }

    #[test]
    fn statement() {
        let mut parser = Parser::new("include \"hello\"".chars());
        assert_eq!(parser.parse_statement().unwrap(), Span {
            item: Statement::Include("hello".into()),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 14}
        });

        let mut parser = Parser::new("hello = \"hello\"".chars());
        assert_eq!(parser.parse_statement().unwrap(), Span {
            item: Statement::Assign(Path {
                path_type: PathType::Local,
                path: vec!["hello".into()]
            }, Value::String("hello".into())),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 14}
        });

        let mut parser = Parser::new("hello.world = \"hello\"".chars());
        assert_eq!(parser.parse_statement().unwrap(), Span {
            item: Statement::Assign(Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            }, Value::String("hello".into())),
            from: Position { line: 0, column: 0},
            to: Position { line: 0, column: 20}
        });
        
        let mut parser = Parser::new("include hello".chars());
        assert!(parser.parse_statement().is_err());
        let mut parser = Parser::new("hello hello".chars());
        assert!(parser.parse_statement().is_err());
        let mut parser = Parser::new("include.hello = \"hello\"".chars());
        assert!(parser.parse_statement().is_err());
        let mut parser = Parser::new("include = \"hello\"".chars());
        assert!(parser.parse_statement().is_err());
        let mut parser = Parser::new("hello.include = \"hello\"".chars());
        assert!(parser.parse_statement().is_err());
        let mut parser = Parser::new("hello = include".chars());
        assert!(parser.parse_statement().is_err());
    }
}
