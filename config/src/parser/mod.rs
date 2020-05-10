use crate::ast;
use lalrpop_util::{lexer::Token, ParseError};

use crate::grammar;

pub type Error<'s> = ParseError<usize, Token<'s>, &'s str>;

pub fn parse(src: &str) -> Result<Vec<ast::Statement>, Error<'_>> {
    grammar::StatementsParser::new()
        .parse(src)
        .map_err(|e| e.into())
}

#[cfg(test)]
mod tests {
    use crate::ast::{Path, PathType, Statement, Value};
    use crate::parser::parse;

    #[test]
    fn statement() {
        let stmt = parse("include \"hello\"");
        assert_eq!(
            stmt.unwrap(),
            vec![Statement::Include("hello".into(), None)]
        );

        let stmt = parse("include \"hello\" as a.b.c");
        assert_eq!(
            stmt.unwrap(),
            vec![Statement::Include(
                "hello".into(),
                Some(Path {
                    path_type: PathType::Local,
                    path: vec!["a".into(), "b".into(), "c".into()]
                })
            )]
        );

        let stmt = parse("hello = \"hello\"");
        assert_eq!(
            stmt.unwrap(),
            vec![Statement::Assign(
                Path {
                    path_type: PathType::Local,
                    path: vec!["hello".into()]
                },
                Value::String("hello".into())
            )]
        );

        let stmt = parse("hello.world = \"hello\"");
        assert_eq!(
            stmt.unwrap(),
            vec![Statement::Assign(
                Path {
                    path_type: PathType::Local,
                    path: vec!["hello".into(), "world".into()]
                },
                Value::String("hello".into())
            )]
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
