#![cfg_attr(not(feature = "std"), no_std)]

pub mod api;
pub mod grammar;
pub mod lexer;
pub mod parser;

pub use arena::Arena;

// Choose between std and alloc
cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        extern crate std;
        use std::prelude::v1::*;

        pub type Map<K, V> = std::collections::HashMap<K, V>;
    } else {
        extern crate alloc;
        pub use alloc::string::*;
        pub use alloc::vec::*;
        use alloc::vec;

        pub type Map<K, V> = alloc::collections::BTreeMap<K, V>;
    }
}

#[derive(Debug, Clone)]
pub struct Parser<'a> {
    pub lexer: lexer::Lexer,
    pub grammar: grammar::Grammar<'a>,
    pub parser: parser::Parser,
}

impl<'a> Default for Parser<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Parser<'a> {
    pub fn new() -> Parser<'a> {
        let lexer = lexer::Lexer::new();
        let grammar = grammar::Grammar::new();
        Parser {
            lexer,
            grammar,
            parser: parser::Parser::new(),
        }
    }

    pub fn parse(
        &'a self,
        tokens: &Vec<lexer::Token>,
        text: &'a str,
    ) -> Result<parser::ParseResult<'a>, parser::ParseError<'a>> {
        self.parser.parse(&self.grammar, &self.lexer, text, tokens)
    }
}

#[cfg(test)]
#[cfg(feature = "std")]
mod tests {

    use core::panic;

    use arena::Arena;

    use crate::{
        api::ext::{enumerator, local, node, text, token, word},
        lexer::TokenKinds,
    };

    use self::grammar::{Parameters, VariableKind};

    use super::*;

    #[test]
    fn arithmetic_tokens() {
        let mut parser = Parser::new();
        let txt = "Function 1 +\n 2 * 3 - 4 /= 5";
        // Tokens that will be recognized by the lexer
        //
        // White space is ignored by default
        //
        // Everything else is a text token
        parser
            .lexer
            .add_tokens(["+", "-", "*", "/=", "Function"].into_iter());

        // Parse the text
        let tokens = parser.lexer.lex_utf8(txt).unwrap();

        assert_eq!(tokens.len(), 21);
    }

    #[test]
    fn stringify() {
        let mut parser = Parser::new();
        let txt = "Functiond\t 1 +\n 2 * 3 - 4 /= 5";
        // Tokens that will be recognized by the lexer
        //
        // White space is ignored by default
        //
        // Everything else is a text token
        parser
            .lexer
            .add_tokens(["+", "-", "*", "/=", "Function"].into_iter());

        // Parse the text
        let tokens = parser.lexer.lex_utf8(txt).unwrap();

        assert_eq!(parser.lexer.stringify_slice(&tokens, txt), txt);
        assert_eq!(parser.lexer.stringify_slice(&tokens[0..1], txt), "Function");
        assert_eq!(parser.lexer.stringify_slice(&tokens[1..5], txt), "d\t 1");
    }

    #[test]
    fn unfinished_token() {
        let mut parser = Parser::new();
        let txt = "fun";
        parser.lexer.add_token("function");
        let tokens = parser.lexer.lex_utf8(txt).unwrap();
        assert_eq!(tokens[0].kind, TokenKinds::Text);
    }

    #[test]
    fn rules() {
        use crate::api::ext;

        let mut parser = Parser::new();
        let txt = "let   danda:hhh=  1+60;";
        parser.lexer.add_token("=");
        parser.lexer.add_token(":");
        parser.lexer.add_token("+");
        parser.lexer.add_token(";");
        parser.lexer.add_token("-");
        parser.lexer.add_token("*");
        parser.lexer.add_token("/");

        let tokens = parser.lexer.lex_utf8(txt).unwrap();

        let operators = parser.grammar.enumerators.push(grammar::Enumerator {
            name: "operators",
            values: [token("+"), token("-"), token("*"), token("/")].to_vec(),
        });
        let value = parser.grammar.add_node(grammar::Node {
            name: "value",
            rules: ext::rules([
                ext::is(text()).set(local("nodes")),
                ext::while_(enumerator(operators))
                    .set(local("nodes"))
                    .then([ext::is(text()).set(local("nodes"))]),
            ]),
            variables: [("nodes", VariableKind::NodeList)].to_vec(),
            docs: None,
        });

        let kw_let = parser.grammar.add_node(grammar::Node {
            name: "KWLet",
            rules: ext::rules([
                ext::is(word("let")).hard_err(),
                ext::is(text()).set(local("ident")),
                ext::maybe(token(":")).then([ext::is(text()).set(local("type"))]),
                ext::maybe(token("=")).then([ext::is(node(value)).set(local("value"))]),
                ext::maybe(token(";")),
            ]),
            variables: [
                ("ident", VariableKind::Node),
                ("type", VariableKind::Node),
                ("value", VariableKind::Node),
            ]
            .to_vec(),
            docs: Some("let <ident>[: <type>] [= <value>];"),
        });
        parser.parser.entry = Some(kw_let);

        let valid = parser.grammar.validate(&parser.lexer);
        if !valid.success() {
            for warn in valid.warnings {
                println!("{}", warn);
            }
            for err in valid.errors {
                println!("{}", err);
            }
            panic!()
        }

        match parser.parse(&tokens, txt) {
            Ok(res) => {
                let entry = res.entry;
                let ident = parser.lexer.stringify(
                    entry
                        .variables
                        .get("ident")
                        .unwrap()
                        .unwrap_node()
                        .unwrap_token(),
                    txt,
                );
                print!("result: let {ident}");
                let let_type = parser.lexer.stringify(
                    entry
                        .variables
                        .get("type")
                        .unwrap()
                        .unwrap_node() // panics if no type
                        .unwrap_token(),
                    txt,
                );
                print!(": {let_type}");
                print!(";");

                panic!()
            }
            Err(e) => {
                panic!("{e}");
            }
        }
    }

    //     #[test]
    //     fn string() {
    //         let txt = r#"

    // "úťf-8 štring"
    // "second string"
    // "#;

    //         let mut parser = Parser::new();
    //         parser.lexer.add_token("\"".into());

    //         // add random tokens to test the lexer
    //         parser.lexer.add_token("=".into());
    //         parser.lexer.add_token(";".into());
    //         parser.lexer.add_token(":".into());
    //         parser.lexer.add_token("+".into());
    //         parser.lexer.add_token("-".into());
    //         parser.lexer.add_token("*".into());
    //         parser.lexer.add_token("/".into());
    //         parser.lexer.add_token("let".into());
    //         parser.lexer.add_token("function".into());
    //         parser.lexer.add_token("danda".into());
    //         parser.lexer.add_token("1".into());
    //         parser.lexer.add_token("60".into());
    //         parser.lexer.add_token("string".into());
    //         parser.lexer.add_token(" ".into());

    //         let tokens = parser.lexer.lex_utf8(txt).unwrap();

    //         let mut variables = Map::new();
    //         variables.insert("start".into(), VariableKind::Node);
    //         variables.insert("end".into(), VariableKind::Node);
    //         parser.grammar.add_node(grammar::Node {
    //             name: "string",
    //             rules: vec![
    //                 // detect the start
    //                 grammar::Rule::Is {
    //                     token: grammar::MatchToken::Token(TokenKinds::Token("\"".into())),
    //                     rules: vec![],
    //                     parameters: vec![Parameters::Set("start"), Parameters::NodeStart],
    //                 },
    //                 grammar::Rule::Until {
    //                     token: grammar::MatchToken::Token(TokenKinds::Token("\"".into())),
    //                     rules: vec![],
    //                     parameters: vec![Parameters::Set("end"), Parameters::NodeEnd],
    //                 },
    //             ],
    //             variables,
    //             docs: Some("string"),
    //         });

    //         let mut variables = Map::new();
    //         variables.insert("strings".into(), VariableKind::NodeList);
    //         variables.insert("count".into(), VariableKind::Number);
    //         variables.insert("zero".into(), VariableKind::Number);

    //         parser.grammar.add_node(grammar::Node {
    //             name: "entry",
    //             rules: vec![
    //                 grammar::Rule::While {
    //                     token: grammar::MatchToken::Node("string"),
    //                     rules: vec![],
    //                     parameters: vec![Parameters::Set("strings"), Parameters::Increment("count")],
    //                 },
    //                 grammar::Rule::Command {
    //                     command: grammar::Commands::Compare {
    //                         left: "count",
    //                         right: "zero", // zero is not defined, so it will be 0
    //                         comparison: grammar::Comparison::Equal,
    //                         rules: vec![grammar::Rule::Command {
    //                             command: grammar::Commands::Error {
    //                                 message: "No strings found",
    //                             },
    //                         }],
    //                     },
    //                 },
    //             ],
    //             variables,
    //             docs: Some("entry"),
    //         });

    //         let result = parser.parse(&tokens, txt).unwrap();
    //         let strings = result.entry.get_list("strings");
    //         assert_eq!(strings.len(), 2);

    //         // first string
    //         assert_eq!(
    //             ParseResult::stringify_node(&strings[0], txt),
    //             r#""úťf-8 štring""#
    //         );

    //         // second string
    //         assert_eq!(
    //             ParseResult::stringify_node(&strings[1], txt),
    //             r#""second string""#
    //         );
    //     }

    //     #[test]
    //     fn vec_char_eq() {
    //         let a = vec!['a', 'b', 'c'];
    //         let b = vec!['a', 'b', 'c'];
    //         let c = vec!['a', 'b', 'd'];
    //         assert_eq!(a, b);
    //         assert_eq!(true, a == b);
    //         assert_eq!(false, a == c);

    //         let slice_a = &a[0..2];
    //         let slice_b = &b[0..2];
    //         let slice_c = &c[1..3];
    //         assert_eq!(slice_a, slice_b);
    //         assert_eq!(true, slice_a == slice_b);
    //         assert_eq!(false, slice_a == slice_c);
    //     }

    //     /// Fields are ordered according to the order of the lines in the meta file
    //     struct Meta {
    //         lines: usize,
    //         line_length: usize,
    //     }

    //     fn read_dotmeta() -> Meta {
    //         use std::fs;
    //         let meta = fs::read_to_string("workload.meta").unwrap();
    //         let mut lns = meta.lines();
    //         let lines = lns.next().unwrap().parse().unwrap();
    //         let line_length = lns.next().unwrap().parse().unwrap();
    //         Meta { lines, line_length }
    //     }

    //     #[test]
    //     fn workload_file() {
    //         let meta = read_dotmeta();
    //         let mut parser = Parser::new();
    //         // let txt = include_str!("../workload.txt"); // The size of the file is 100MB which would make it impractical to include it in the tests
    //         use std::fs;
    //         let txt = fs::read_to_string("workload.txt").unwrap();
    //         parser.lexer.add_token("\"".into());

    //         let lex_start = std::time::Instant::now();
    //         let tokens = parser.lexer.lex_utf8(&txt).unwrap();

    //         let variables = Map::new();
    //         parser.grammar.add_node(grammar::Node {
    //             name: "string",
    //             rules: vec![
    //                 // detect the start
    //                 grammar::Rule::Is {
    //                     token: grammar::MatchToken::Token(TokenKinds::Token("\"".into())),
    //                     rules: vec![],
    //                     parameters: vec![Parameters::NodeStart, Parameters::HardError(true)],
    //                 },
    //                 grammar::Rule::Until {
    //                     token: grammar::MatchToken::Token(TokenKinds::Token("\"".into())),
    //                     rules: vec![],
    //                     parameters: vec![Parameters::NodeEnd],
    //                 },
    //             ],
    //             variables,
    //             docs: Some("string"),
    //         });

    //         let mut variables = Map::new();
    //         variables.insert("strings".into(), VariableKind::NodeList);
    //         variables.insert("count".into(), VariableKind::Number);
    //         variables.insert("zero".into(), VariableKind::Number);

    //         parser.grammar.add_node(grammar::Node {
    //             name: "entry",
    //             rules: vec![
    //                 grammar::Rule::While {
    //                     token: grammar::MatchToken::Node("string"),
    //                     rules: vec![],
    //                     parameters: vec![Parameters::Set("strings"), Parameters::Increment("count")],
    //                 },
    //                 grammar::Rule::Command {
    //                     command: grammar::Commands::Compare {
    //                         left: "count",
    //                         right: "zero", // zero is not defined, so it will be 0
    //                         comparison: grammar::Comparison::Equal,
    //                         rules: vec![grammar::Rule::Command {
    //                             command: grammar::Commands::Error {
    //                                 message: "No strings found",
    //                             },
    //                         }],
    //                     },
    //                 },
    //             ],
    //             variables,
    //             docs: Some("entry"),
    //         });

    //         let result = parser.parse(&tokens, &txt).unwrap();
    //         let strings = result.entry.get_list("strings");
    //         // verify the result
    //         assert_eq!(strings.len(), meta.lines);
    //         for s in strings {
    //             assert_eq!(ParseResult::stringify_node(s, &txt).len(), meta.line_length);
    //         }
    //     }

    // #[test]
    // fn load_json() {
    //     use std::io::Read;

    //     let mut file = std::fs::File::open("KWLet.json").unwrap();
    //     let mut parser = String::new();
    //     file.read_to_string(&mut parser).unwrap();

    //     let parser: Parser = serde_json::from_str(&parser).unwrap();

    //     let txt = "let a: int = 500 * 9;";

    //     let tokens = parser.lexer.lex_utf8(txt).unwrap();

    //     let result = parser.parse(&tokens, txt).unwrap();

    //     assert_eq!(
    //         ParseResult::stringify_node(result.entry.try_get_node("value").as_ref().unwrap(), txt),
    //         " 500 * 9"
    //     );
    // }
}
