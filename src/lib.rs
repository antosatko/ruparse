#![cfg_attr(not(feature = "std"), no_std)]

pub mod api;
pub mod grammar;
pub mod lexer;
pub mod parser;

pub mod format;

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
    pub parser: parser::Parser<'a>,
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
    use std::time::Instant;

    use crate::{
        api::ext::{enumerator, local, node, text, token, word},
        grammar::validator::Validator,
        lexer::TokenKinds,
    };

    use self::grammar::VariableKind;

    use super::*;

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

        let txt = "let   danda sagsdfg= sdf;\n\tlet b";

        let mut parser = Parser::new();
        parser
            .lexer
            .add_tokens("=:;+-/*".split("").filter(|s| !s.is_empty()));

        parser.grammar.add_enum(grammar::Enumerator {
            name: "operators",
            values: [token("+"), token("-"), token("*"), token("/")].to_vec(),
        });
        parser.grammar.add_node(grammar::Node {
            name: "value",
            rules: ext::rules([
                ext::is(text()).set(local("nodes")).commit(),
                ext::while_(enumerator("operators"))
                    .set(local("nodes"))
                    .then([ext::is(text()).set(local("nodes"))]),
            ]),
            variables: [("nodes", VariableKind::NodeList)].to_vec(),
            docs: Some("example: 1 + 6 - value1"),
        });

        parser.grammar.add_node(grammar::Node {
            name: "KWLet",
            rules: ext::rules([
                ext::is(word("let")).commit().start(),
                ext::is(text()).set(local("ident")),
                ext::maybe(token(":")).then([ext::is(text()).set(local("type"))]),
                ext::maybe(token("=")).then([ext::is(node("value")).set(local("value"))]),
                ext::is(token(";")).hint("Close let statement with a semicolon"),
            ]),
            variables: [
                ("ident", VariableKind::Node),
                ("type", VariableKind::Node),
                ("value", VariableKind::Node),
            ]
            .to_vec(),
            docs: Some("example: let identifier: Type = value;"),
        });
        parser.grammar.add_node(grammar::Node {
            name: "entry",
            rules: ext::rules([ext::while_(node("KWLet")).set(local("lets"))]),
            variables: [("lets", VariableKind::NodeList)].to_vec(),
            docs: Some("A list of let statements"),
        });
        parser.parser.entry = Some("entry");

        let valid = Validator::default().validate(&parser);
        if !valid.success() {
            valid.print_all().unwrap();
            panic!();
        }
        let tokens = parser.lexer.lex_utf8(txt).unwrap();
        let start_time = Instant::now();
        match parser.parse(&tokens, txt) {
            Ok(res) => {
                println!("Parsing done, duration: {:?}", start_time.elapsed());
                let entry = res.entry;
                for entry in entry.get_list("lets").iter().map(|e| e.unwrap_node()) {
                    let ident = entry
                        .variables
                        .get("ident")
                        .unwrap()
                        .unwrap_node()
                        .stringify(txt);
                    print!("result: let {ident}");
                    if let Some(t) = entry.variables.get("type").unwrap().try_unwrap_node() {
                        let t = t.stringify(txt);
                        print!(": {t}")
                    }
                    if let Some(v) = entry.try_get_node("value") {
                        print!(" =");
                        for node in v.unwrap_node().get_list("nodes") {
                            let v = node.stringify(txt);
                            print!(" {v}");
                        }
                    }
                    println!(";");
                }

                // panic!("All good :)")
            }
            Err(e) => {
                println!(
                    "Parsing ended on an error, duration: {:?}",
                    start_time.elapsed()
                );
                e.print(txt, Some(&format!("{}-test", file!()))).unwrap();
                panic!("");
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
