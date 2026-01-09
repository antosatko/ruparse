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
    pub lexer: lexer::Lexer<'a>,
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

        let txt = "let   danda = sdf;\n\tlet b;";

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
                print!(";");
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
}
