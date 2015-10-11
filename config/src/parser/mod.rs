use lalrpop_util::ParseError;
use ast;

mod grammar;

#[derive(Debug)]
pub enum Error {
    InvalidToken(usize),
    UnrecognizedToken {
        token: Option<(usize, (usize, String), usize)>,
        expected: Vec<String>
    },
    ExtraToken((usize, (usize, String), usize)),
    Other
}

impl<'s> From<ParseError<usize,(usize, &'s str),()>> for Error {
    fn from(e: ParseError<usize,(usize, &'s str),()>) -> Error {
        match e {
            ParseError::InvalidToken { location } => Error::InvalidToken(location),
            ParseError::UnrecognizedToken { token, expected } => Error::UnrecognizedToken {
                token: token.map(|(l1, (n, t), l2)| (l1, (n, t.into()), l2)),
                expected: expected
            },
            ParseError::ExtraToken { token: (l1, (n, t), l2) } => Error::ExtraToken((l1, (n, t.into()), l2)),
            ParseError::User { error: () } => Error::Other
        }
    }
}

pub fn parse(src: &str) -> Result<Vec<ast::Statement>, Error> {
    grammar::parse_Statements(src).map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use ast::{
        Path,
        PathType,
        Value,
        Statement
    };
    use parser::parse;

    #[test]
    fn statement() {
        let stmt = parse("include \"hello\"");
        assert_eq!(stmt.unwrap(), vec![Statement::Include("hello".into())]);

        let stmt = parse("hello = \"hello\"");
        assert_eq!(stmt.unwrap(), vec![Statement::Assign(Path {
                path_type: PathType::Local,
                path: vec!["hello".into()]
            }, Value::String("hello".into()))]
        );

        let stmt = parse("hello.world = \"hello\"");
        assert_eq!(stmt.unwrap(), vec![Statement::Assign(Path {
                path_type: PathType::Local,
                path: vec!["hello".into(), "world".into()]
            }, Value::String("hello".into()))]
        );
        
        let stmt = parse("include hello");
        assert!(stmt.is_err());
        let stmt = parse("hello hello");
        assert!(stmt.is_err());
        let stmt = parse("include.hello = \"hello\"");
        assert!(stmt.is_err());
        let stmt = parse("include = \"hello\"");
        assert!(stmt.is_err());
        let stmt = parse("hello.include = \"hello\"");
        assert!(stmt.is_err());
        let stmt = parse("hello = include");
        assert!(stmt.is_err());
    }
}

