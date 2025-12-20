#![cfg_attr(not(feature = "std"), no_std)]

pub mod api;
pub mod grammar;
pub mod lexer;
pub mod parser;

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

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Parser<'a> {
    pub lexer: lexer::Lexer,
    pub grammar: grammar::Grammar<'a>,
    pub parser: parser::Parser,
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

    use core::fmt::Debug;
    use std::io::Write;

    use arena::Arena;

    use crate::{lexer::TokenKinds, parser::ParseResult};

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
        parser.lexer.add_token("function".into());
        let tokens = parser.lexer.lex_utf8(txt).unwrap();
        assert_eq!(tokens[0].kind, TokenKinds::Text);
    }

    #[test]
    fn rules() {
        let mut parser = Parser::new();
        let txt = "let   danda=  1+60";
        parser.lexer.add_token("=".into());
        parser.lexer.add_token(":".into());
        parser.lexer.add_token("+".into());
        parser.lexer.add_token(";".into());
        parser.lexer.add_token("-".into());
        parser.lexer.add_token("*".into());
        parser.lexer.add_token("/".into());

        let tokens = parser.lexer.lex_utf8(txt).unwrap();

        let operators = parser.grammar.enumerators.push(grammar::Enumerator {
            name: "operators",
            values: vec![
                grammar::MatchToken::Token(TokenKinds::Token("+".into())),
                grammar::MatchToken::Token(TokenKinds::Token("-".into())),
                grammar::MatchToken::Token(TokenKinds::Token("*".into())),
                grammar::MatchToken::Token(TokenKinds::Token("/".into())),
            ],
        });
        let mut variables = Arena::new();
        let nodes = variables.push(VariableKind::NodeList);
        let value = parser.grammar.add_node(grammar::Node {
            name: "value",
            rules: vec![
                // detect the value[0]
                grammar::Rule::Is {
                    token: grammar::MatchToken::Token(TokenKinds::Text),
                    rules: vec![],
                    parameters: vec![Parameters::Set(nodes)],
                },
                // detect the operator
                grammar::Rule::While {
                    token: grammar::MatchToken::Enumerator(operators),
                    // detect the value[n]
                    rules: vec![grammar::Rule::Is {
                        token: grammar::MatchToken::Token(TokenKinds::Text),
                        rules: vec![],
                        parameters: vec![Parameters::Set(nodes)],
                    }],
                    parameters: vec![Parameters::Set(nodes)],
                },
            ],
            variables,
            docs: Some("value"),
        });

        let mut variables = Arena::new();
        let ident = variables.push(VariableKind::Node);
        let type_ = variables.push(VariableKind::Node);
        let value_v = variables.push(VariableKind::Node);
        let kw_let = parser.grammar.add_node(grammar::Node {
            name: "KWLet",
            rules: vec![
                // detect the keyword
                grammar::Rule::Is {
                    token: grammar::MatchToken::Word("let"),
                    rules: vec![],
                    parameters: vec![Parameters::HardError(true)],
                },
                // detect the ident
                grammar::Rule::Is {
                    token: grammar::MatchToken::Token(TokenKinds::Text),
                    rules: vec![],
                    parameters: vec![Parameters::Set(ident)],
                },
                // detect the type if it exists
                grammar::Rule::Maybe {
                    token: grammar::MatchToken::Token(TokenKinds::Token(":".into())),
                    is: vec![grammar::Rule::Is {
                        token: grammar::MatchToken::Token(TokenKinds::Text),
                        rules: vec![],
                        parameters: vec![Parameters::Set(type_)],
                    }],
                    isnt: vec![],
                    parameters: vec![],
                },
                // detect the value if it exists
                grammar::Rule::Maybe {
                    token: grammar::MatchToken::Token(TokenKinds::Token("=".into())),
                    is: vec![grammar::Rule::Is {
                        token: grammar::MatchToken::Node(value),
                        rules: vec![],
                        parameters: vec![Parameters::Set(value_v)],
                    }],
                    isnt: vec![],
                    parameters: vec![],
                },
                // consume the semicolon (optional)
                grammar::Rule::Maybe {
                    token: grammar::MatchToken::Token(TokenKinds::Token(";".into())),
                    is: vec![],
                    isnt: vec![],
                    parameters: vec![],
                },
            ],
            variables,
            docs: Some("let <ident>[: <type>] [= <value>];"),
        });
        parser.parser.entry = Some(kw_let);

        match parser.parse(&tokens, txt) {
            Ok(_) => (),
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
