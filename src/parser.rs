use crate::{
    grammar::{EnumeratorTag, GlobalVariableTag, NodeTag, VarKind, VariableTag},
    Map,
};

use arena::{Arena, Key};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

const DEFAULT_ENTRY: &str = "entry";

use crate::{
    grammar::{self, Grammar, MatchToken, OneOf},
    lexer::{Lexer, TextLocation, Token, TokenKinds},
};

// Choose between std and alloc
cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        extern crate std;
        use std::prelude::v1::*;
        use std::fmt;
    } else {
        extern crate alloc;
        use alloc::string::*;
        use alloc::vec::*;
        use alloc::vec;
        use core::fmt;
        use alloc::format;
    }
}

#[derive(Debug, Clone)]
pub struct Parser {
    pub entry: Option<Key<NodeTag>>,
    /// Option to enable error on eof
    pub eof_error: bool,
}

impl Parser {
    pub fn new() -> Parser {
        Parser {
            entry: None,
            eof_error: false,
        }
    }

    pub(crate) fn parse<'a>(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        text: &'a str,
        tokens: &Vec<Token>,
    ) -> Result<ParseResult<'a>, ParseError<'a>> {
        let mut cursor = Cursor {
            idx: 0,
            to_advance: false,
        };
        let entry = match &self.entry {
            Some(e) => e,
            None => {
                return Err(ParseError {
                    kind: ParseErrors::MissingEntry,
                    location: TextLocation::new(0, 0),
                    node: None,
                })
            }
        };
        let mut globals = Node::variables_from_grammar(&grammar.globals)?;
        let entry = match self.parse_node(
            grammar,
            lexer,
            entry,
            &mut cursor,
            &mut globals,
            tokens,
            text,
        ) {
            Ok(node) => {
                if !grammar.eof {
                    node
                } else {
                    if cursor.to_advance {
                        cursor.to_advance = false;
                        cursor.idx += 1;
                    }
                    // If the grammar has an eof token, we need to check if the cursor is at the end of the tokens
                    // Consume all the whitespace tokens
                    while cursor.idx < tokens.len() && tokens[cursor.idx].kind.is_whitespace() {
                        cursor.idx += 1;
                    }
                    if let TokenKinds::Control(crate::lexer::ControlTokenKind::Eof) =
                        tokens[cursor.idx].kind
                    {
                        node
                    } else {
                        return Err(ParseError {
                            kind: ParseErrors::MissingEof(tokens[cursor.idx].kind.clone()),
                            location: tokens[cursor.idx].location.clone(),
                            node: Some(node),
                        });
                    }
                }
            }
            Err((err, _)) => return Err(err),
        };

        Ok(ParseResult { entry, globals })
    }

    fn parse_node<'a>(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        name: &Key<NodeTag>,
        cursor: &mut Cursor,
        globals: &mut Arena<VariableKind<'a>, GlobalVariableTag>,
        tokens: &Vec<Token>,
        text: &'a str,
    ) -> Result<Node<'a>, (ParseError<'a>, Node<'a>)> {
        #[cfg(feature = "debug")]
        println!("-- start: {}, cursor: {:?}", name, cursor);
        let mut node = match Node::from_grammar(grammar, name) {
            Ok(node) => node,
            Err(err) => return Err((err, Node::new("<dummy node>"))),
        };
        node.first_string_idx = tokens[cursor.idx].index;
        // In case the node fails to parse, we want to restore the cursor to its original position
        let cursor_clone = cursor.clone();
        let rules = match grammar.nodes.get(name) {
            Some(node) => &node.rules,
            None => {
                return Err((
                    ParseError {
                        kind: ParseErrors::NodeNotFound(*name),
                        location: tokens[cursor.idx].location.clone(),
                        node: Some(node.clone()),
                    },
                    node,
                ))
            }
        };
        let result = self.parse_rules(
            grammar,
            lexer,
            rules,
            cursor,
            globals,
            &cursor_clone,
            &mut node,
            tokens,
            text,
        );

        #[cfg(feature = "debug")]
        println!("-- end: {}, cursor: {:?}", name, cursor);

        // If the node has not set the last_string_idx, we set it to the end of the last token
        if node.last_string_idx == 0 {
            if cursor.idx >= tokens.len() {
                node.last_string_idx = tokens.last().unwrap().index + tokens.last().unwrap().len;
            } else {
                node.last_string_idx = tokens[cursor.idx].index + tokens[cursor.idx].len;
            }
        }

        match result {
            Ok(ref msg) => match msg {
                Msg::Ok => Ok(node),
                Msg::Return => Ok(node),
                Msg::Break(n) => Err((
                    ParseError {
                        kind: ParseErrors::CannotBreak(*n),
                        location: tokens[cursor.idx].location.clone(),
                        node: Some(node.clone()),
                    },
                    node,
                )),
                Msg::Back(steps) => Err((
                    ParseError {
                        kind: ParseErrors::CannotGoBack(*steps),
                        location: tokens[cursor.idx].location.clone(),
                        node: Some(node.clone()),
                    },
                    node,
                )),
                Msg::Goto(label) => Err((
                    ParseError {
                        kind: ParseErrors::LabelNotFound(label.to_string()),
                        location: tokens[cursor.idx].location.clone(),
                        node: Some(node.clone()),
                    },
                    node,
                )),
            },
            Err(err) => {
                #[cfg(feature = "debug")]
                println!("error: {:?}", err);
                *cursor = cursor_clone;
                Err((err, node))
            }
        }
    }

    fn parse_rules<'a>(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        rules: &'a Vec<grammar::Rule<'a>>,
        cursor: &mut Cursor,
        globals: &mut Arena<VariableKind<'a>, GlobalVariableTag>,
        cursor_clone: &Cursor,
        node: &mut Node<'a>,
        tokens: &Vec<Token>,
        text: &'a str,
    ) -> Result<Msg, ParseError<'a>> {
        let mut advance = true;
        let mut msg_bus = MsgBus::new();
        let mut i = 0;
        while i < rules.len() {
            let rule = &rules[i];
            if cursor.to_advance {
                cursor.to_advance = false;
                cursor.idx += 1;
                if cursor.idx >= tokens.len() {
                    if self.eof_error {
                        return Err(ParseError {
                            kind: ParseErrors::Eof,
                            location: tokens[cursor.idx - 1].location.clone(),
                            node: Some(node.clone()),
                        });
                    } else {
                        cursor.idx -= 1;
                    }
                }
            }
            #[cfg(feature = "debug")]
            println!(
                "tok: <{}> kind: {:?} -- parent: {}",
                lexer.stringify(&tokens[cursor.idx], text),
                tokens[cursor.idx].kind,
                node.name
            );
            #[cfg(feature = "debug")]
            println!("rule: {:?}", rule);
            // stringifying the token
            match rule {
                grammar::Rule::Is {
                    token,
                    rules,
                    parameters,
                } => {
                    match self.match_token(
                        grammar,
                        lexer,
                        token,
                        cursor,
                        globals,
                        cursor_clone,
                        tokens,
                        text,
                    )? {
                        TokenCompare::Is(val) => {
                            let is_token = val.is_token();
                            self.parse_parameters(
                                grammar,
                                lexer,
                                parameters,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                &val,
                                &mut msg_bus,
                                tokens,
                                text,
                            )?;
                            if is_token {
                                cursor.to_advance = true;
                            }
                            self.parse_rules(
                                grammar,
                                lexer,
                                rules,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                tokens,
                                text,
                            )?
                            .push(&mut msg_bus);
                        }
                        TokenCompare::IsNot(err) => {
                            return Err(err);
                        }
                    };
                }
                grammar::Rule::Isnt {
                    token,
                    rules,
                    parameters: _,
                } => {
                    match self.match_token(
                        grammar,
                        lexer,
                        token,
                        cursor,
                        globals,
                        cursor_clone,
                        tokens,
                        text,
                    )? {
                        TokenCompare::Is(_) => {
                            err(
                                ParseErrors::ExpectedToNotBe(tokens[cursor.idx].kind.clone()),
                                cursor,
                                cursor_clone,
                                &tokens[cursor.idx].location,
                                Some(node.clone()),
                            )?;
                        }
                        TokenCompare::IsNot(_) => {
                            self.parse_rules(
                                grammar,
                                lexer,
                                rules,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                tokens,
                                text,
                            )?
                            .push(&mut msg_bus);
                        }
                    }
                }
                grammar::Rule::IsOneOf { tokens: pos_tokens } => {
                    let mut found = false;
                    for OneOf {
                        token,
                        rules,
                        parameters,
                    } in pos_tokens
                    {
                        use TokenCompare::*;
                        #[cfg(feature = "debug")]
                        println!("trying option: {:?}", token);
                        match self.match_token(
                            grammar,
                            lexer,
                            &token,
                            cursor,
                            globals,
                            cursor_clone,
                            tokens,
                            text,
                        )? {
                            Is(val) => {
                                #[cfg(feature = "debug")]
                                println!("success");
                                found = true;
                                let is_token = val.is_token();
                                self.parse_parameters(
                                    grammar,
                                    lexer,
                                    parameters,
                                    cursor,
                                    globals,
                                    cursor_clone,
                                    node,
                                    &val,
                                    &mut msg_bus,
                                    tokens,
                                    text,
                                )?;
                                if is_token {
                                    cursor.to_advance = true;
                                }
                                self.parse_rules(
                                    grammar,
                                    lexer,
                                    rules,
                                    cursor,
                                    globals,
                                    cursor_clone,
                                    node,
                                    tokens,
                                    text,
                                )?
                                .push(&mut msg_bus);
                                break;
                            }
                            IsNot(err) => match err.node {
                                Some(ref node) => {
                                    if node.harderror {
                                        #[cfg(feature = "debug")]
                                        println!("non recoverable error: {:?}", err);
                                        return Err(err);
                                    }
                                }
                                None => {
                                    #[cfg(feature = "debug")]
                                    println!("recoverable error: {:?}", err);
                                    cursor.to_advance = false;
                                }
                            },
                        }
                    }
                    if !found {
                        err(
                            ParseErrors::ExpectedOneOf {
                                expected: pos_tokens.iter().map(|x| x.token.clone()).collect(),
                                found: tokens[cursor.idx].kind.clone(),
                            },
                            cursor,
                            cursor_clone,
                            &tokens[cursor.idx].location,
                            Some(node.clone()),
                        )?;
                    }
                }
                grammar::Rule::Maybe {
                    token,
                    is,
                    isnt,
                    parameters,
                } => {
                    use TokenCompare::*;
                    match self.match_token(
                        grammar,
                        lexer,
                        token,
                        cursor,
                        globals,
                        cursor_clone,
                        tokens,
                        text,
                    )? {
                        Is(val) => {
                            let is_token = val.is_token();
                            self.parse_parameters(
                                grammar,
                                lexer,
                                parameters,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                &val,
                                &mut msg_bus,
                                tokens,
                                text,
                            )?;
                            if is_token {
                                cursor.to_advance = true;
                            }
                            self.parse_rules(
                                grammar,
                                lexer,
                                is,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                tokens,
                                text,
                            )?
                            .push(&mut msg_bus);
                        }
                        IsNot(err) => {
                            match err.node {
                                Some(ref node) => {
                                    if node.harderror {
                                        return Err(err);
                                    }
                                }
                                None => (),
                            }
                            self.parse_rules(
                                grammar,
                                lexer,
                                isnt,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                tokens,
                                text,
                            )?
                            .push(&mut msg_bus);
                        }
                    }
                }
                grammar::Rule::MaybeOneOf { is_one_of, isnt } => {
                    let mut found = false;
                    for OneOf {
                        token,
                        rules,
                        parameters,
                    } in is_one_of
                    {
                        use TokenCompare::*;
                        match self.match_token(
                            grammar,
                            lexer,
                            &token,
                            cursor,
                            globals,
                            cursor_clone,
                            tokens,
                            text,
                        )? {
                            Is(val) => {
                                found = true;
                                let is_token = val.is_token();
                                self.parse_parameters(
                                    grammar,
                                    lexer,
                                    parameters,
                                    cursor,
                                    globals,
                                    cursor_clone,
                                    node,
                                    &val,
                                    &mut msg_bus,
                                    tokens,
                                    text,
                                )?;
                                #[cfg(feature = "debug")]
                                println!("is_token: {}", is_token);
                                if is_token {
                                    cursor.to_advance = true;
                                }
                                self.parse_rules(
                                    grammar,
                                    lexer,
                                    rules,
                                    cursor,
                                    globals,
                                    cursor_clone,
                                    node,
                                    tokens,
                                    text,
                                )?
                                .push(&mut msg_bus);
                                break;
                            }
                            IsNot(err) => match err.node {
                                Some(ref node) => {
                                    if node.harderror {
                                        return Err(err);
                                    }
                                }
                                None => (),
                            },
                        }
                    }
                    if !found {
                        self.parse_rules(
                            grammar,
                            lexer,
                            isnt,
                            cursor,
                            globals,
                            cursor_clone,
                            node,
                            tokens,
                            text,
                        )?
                        .push(&mut msg_bus);
                    }
                }
                grammar::Rule::While {
                    token,
                    rules,
                    parameters,
                } => {
                    match self.match_token(
                        grammar,
                        lexer,
                        token,
                        cursor,
                        globals,
                        cursor_clone,
                        tokens,
                        text,
                    )? {
                        TokenCompare::Is(val) => {
                            let is_token = val.is_token();
                            self.parse_parameters(
                                grammar,
                                lexer,
                                parameters,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                &val,
                                &mut msg_bus,
                                tokens,
                                text,
                            )?;
                            if is_token {
                                cursor.to_advance = true;
                            }
                            self.parse_rules(
                                grammar,
                                lexer,
                                rules,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                tokens,
                                text,
                            )?
                            .push(&mut msg_bus);
                            advance = false;
                        }
                        TokenCompare::IsNot(err) => match err.node {
                            Some(ref node) => {
                                if node.harderror {
                                    return Err(err);
                                }
                            }
                            None => (),
                        },
                    }
                    #[cfg(feature = "debug")]
                    println!("WHILE DONE, CURSOR.TO_ADVANCE = {}", cursor.to_advance);
                    #[cfg(feature = "debug")]
                    println!("\t - WHILE DONE, CURSOR.IDX = {}", cursor.idx);
                }
                grammar::Rule::Until {
                    token,
                    rules,
                    parameters,
                } => {
                    // search for the token and execute the rules when the token is found
                    while let TokenCompare::IsNot(_) = self.match_token(
                        grammar,
                        lexer,
                        token,
                        cursor,
                        globals,
                        cursor_clone,
                        tokens,
                        text,
                    )? {
                        // No need to handle the error here
                        cursor.idx += 1;
                        if cursor.idx >= tokens.len() {
                            return Err(ParseError {
                                kind: ParseErrors::CouldNotFindToken(token.clone()),
                                location: tokens[cursor.idx - 1].location.clone(),
                                node: Some(node.clone()),
                            });
                        }
                    }
                    self.parse_parameters(
                        grammar,
                        lexer,
                        parameters,
                        cursor,
                        globals,
                        cursor_clone,
                        node,
                        &Nodes::Token(tokens[cursor.idx].clone()),
                        &mut msg_bus,
                        tokens,
                        text,
                    )?;
                    cursor.to_advance = true;
                    self.parse_rules(
                        grammar,
                        lexer,
                        rules,
                        cursor,
                        globals,
                        cursor_clone,
                        node,
                        tokens,
                        text,
                    )?
                    .push(&mut msg_bus);
                }
                grammar::Rule::Command { command } => match command {
                    grammar::Commands::Compare {
                        left,
                        right,
                        comparison,
                        rules,
                    } => {
                        use VarKind::*;
                        let left_opt = match left {
                            Local(l) => node.variables.get(l),
                            Global(g) => globals.get(g),
                        };
                        let left = match left_opt {
                            None => {
                                return Err(ParseError {
                                    kind: ParseErrors::VariableNotFound(*left),
                                    location: tokens[cursor.idx].location.clone(),
                                    node: Some(node.clone()),
                                })
                            }
                            Some(v) => v,
                        };
                        let right_opt = match right {
                            Local(l) => node.variables.get(l),
                            Global(g) => globals.get(g),
                        };
                        let right = match right_opt {
                            None => {
                                return Err(ParseError {
                                    kind: ParseErrors::VariableNotFound(*right),
                                    location: tokens[cursor.idx].location.clone(),
                                    node: Some(node.clone()),
                                })
                            }
                            Some(v) => v,
                        };
                        let comparisons = match left {
                            VariableKind::Node(node_left) => {
                                if let VariableKind::Node(node_right) = right {
                                    match (node_left, node_right) {
                                        (Some(Nodes::Node(left)), Some(Nodes::Node(right))) => {
                                            if left.name == right.name {
                                                vec![grammar::Comparison::Equal]
                                            } else {
                                                vec![grammar::Comparison::NotEqual]
                                            }
                                        }
                                        (Some(Nodes::Token(left)), Some(Nodes::Token(right))) => {
                                            if left == right {
                                                vec![grammar::Comparison::Equal]
                                            } else {
                                                vec![grammar::Comparison::NotEqual]
                                            }
                                        }
                                        (None, None) => {
                                            vec![grammar::Comparison::Equal]
                                        }
                                        _ => {
                                            vec![grammar::Comparison::NotEqual]
                                        }
                                    }
                                } else {
                                    vec![grammar::Comparison::NotEqual]
                                }
                            }
                            VariableKind::NodeList(_) => vec![grammar::Comparison::NotEqual],
                            VariableKind::Boolean(left) => {
                                if let VariableKind::Boolean(right) = right {
                                    if left == right {
                                        vec![grammar::Comparison::Equal]
                                    } else {
                                        vec![grammar::Comparison::NotEqual]
                                    }
                                } else {
                                    vec![grammar::Comparison::NotEqual]
                                }
                            }
                            VariableKind::Number(left) => {
                                if let VariableKind::Number(right) = right {
                                    let mut result = Vec::new();
                                    if left == right {
                                        result.push(grammar::Comparison::Equal);
                                        result.push(grammar::Comparison::GreaterThanOrEqual);
                                        result.push(grammar::Comparison::LessThanOrEqual);
                                    } else {
                                        result.push(grammar::Comparison::NotEqual);
                                        if left > right {
                                            result.push(grammar::Comparison::GreaterThan);
                                            result.push(grammar::Comparison::GreaterThanOrEqual);
                                        }
                                        if left < right {
                                            result.push(grammar::Comparison::LessThan);
                                            result.push(grammar::Comparison::LessThanOrEqual);
                                        }
                                    }
                                    result
                                } else {
                                    vec![grammar::Comparison::NotEqual]
                                }
                            }
                        };
                        if comparisons.contains(comparison) {
                            self.parse_rules(
                                grammar,
                                lexer,
                                rules,
                                cursor,
                                globals,
                                cursor_clone,
                                node,
                                tokens,
                                text,
                            )?
                            .push(&mut msg_bus);
                        }
                    }
                    grammar::Commands::Error { message } => Err(ParseError {
                        kind: ParseErrors::Message(message),
                        location: tokens[cursor.idx].location.clone(),
                        node: Some(node.clone()),
                    })?,
                    grammar::Commands::HardError { set } => {
                        node.harderror = *set;
                    }
                    grammar::Commands::Goto { label } => {
                        msg_bus.send(Msg::Goto(label.to_string()));
                    }
                    grammar::Commands::Label { name: _ } => (),
                    grammar::Commands::Print { message: _msg } => {
                        #[cfg(feature = "std")]
                        println!("{}", _msg)
                    }
                },
                grammar::Rule::Loop { rules } => {
                    self.parse_rules(
                        grammar,
                        lexer,
                        rules,
                        cursor,
                        globals,
                        cursor_clone,
                        node,
                        tokens,
                        text,
                    )?
                    .push(&mut msg_bus);
                    advance = false;
                }
                grammar::Rule::UntilOneOf {
                    tokens: match_tokens,
                } => {
                    let mut found = false;
                    while cursor.idx < tokens.len() {
                        for OneOf {
                            token,
                            rules,
                            parameters,
                        } in match_tokens
                        {
                            use TokenCompare::*;
                            match self.match_token(
                                grammar,
                                lexer,
                                token,
                                cursor,
                                globals,
                                cursor_clone,
                                tokens,
                                text,
                            )? {
                                Is(val) => {
                                    found = true;
                                    let is_token = val.is_token();
                                    self.parse_parameters(
                                        grammar,
                                        lexer,
                                        parameters,
                                        cursor,
                                        globals,
                                        cursor_clone,
                                        node,
                                        &val,
                                        &mut msg_bus,
                                        tokens,
                                        text,
                                    )?;
                                    if is_token {
                                        cursor.to_advance = true;
                                    }
                                    self.parse_rules(
                                        grammar,
                                        lexer,
                                        rules,
                                        cursor,
                                        globals,
                                        cursor_clone,
                                        node,
                                        tokens,
                                        text,
                                    )?
                                    .push(&mut msg_bus);
                                    break;
                                }
                                IsNot(err) => match err.node {
                                    Some(ref node) => {
                                        if node.harderror {
                                            return Err(err);
                                        }
                                    }
                                    None => (),
                                },
                            }
                        }
                        if found {
                            break;
                        }
                        cursor.idx += 1;
                    }
                    if !found {
                        err(
                            ParseErrors::ExpectedOneOf {
                                expected: match_tokens.iter().map(|x| x.token.clone()).collect(),
                                found: tokens[cursor.idx].kind.clone(),
                            },
                            cursor,
                            cursor_clone,
                            &tokens[cursor.idx].location,
                            Some(node.clone()),
                        )?;
                    }
                }
                grammar::Rule::Debug { target } => {
                    #[cfg(feature = "std")]
                    {
                        match target {
                            Some(ident) => {
                                // let kind = match node.variables.get(ident) {
                                //     Some(kind) => kind,
                                //     None => {
                                //         return Err(ParseError {
                                //             kind: ParseErrors::VariableNotFound(ident.to_string()),
                                //             location: tokens[cursor.idx].location.clone(),
                                //             node: Some(node.clone()),
                                //         })
                                //     }
                                // };
                                // println!("{:?}", kind);
                            }
                            None => {
                                if cursor.idx >= tokens.len() {
                                    println!("Eof");
                                } else {
                                    println!("{:?}", lexer.stringify(&tokens[cursor.idx], text));
                                }
                            }
                        }
                    }
                }
            }
            if advance {
                i += 1;
            } else {
                advance = true;
            }
            while let Some(msg) = msg_bus.receive() {
                match msg {
                    Msg::Return => return Ok(Msg::Return),
                    Msg::Break(n) => {
                        return if n == 1 {
                            Ok(Msg::Ok)
                        } else {
                            Ok(Msg::Break(n - 1))
                        }
                    }

                    Msg::Goto(label) => {
                        let mut j = 0;
                        loop {
                            if j >= rules.len() {
                                return Ok(Msg::Goto(label));
                            }
                            match &rules[j] {
                                grammar::Rule::Command {
                                    command: grammar::Commands::Label { name },
                                } => {
                                    if *name == label {
                                        i = j;
                                        break;
                                    }
                                }
                                _ => {}
                            }
                            j += 1;
                        }
                    }
                    Msg::Back(steps) => {
                        if i < steps {
                            return Ok(Msg::Back(steps - i));
                        }
                        i -= steps;
                    }
                    Msg::Ok => {}
                }
            }
        }
        Ok(Msg::Ok)
    }

    fn match_token<'a>(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        token: &grammar::MatchToken,
        cursor: &mut Cursor,
        globals: &mut Arena<VariableKind<'a>, GlobalVariableTag>,
        cursor_clone: &Cursor,
        tokens: &Vec<Token>,
        text: &'a str,
    ) -> Result<TokenCompare<'a>, ParseError<'a>> {
        match token {
            grammar::MatchToken::Token(tok) => {
                if *tok == TokenKinds::Control(crate::lexer::ControlTokenKind::Eof) {
                    if cursor.idx >= tokens.len() {
                        return Ok(TokenCompare::Is(Nodes::Token(Token {
                            kind: TokenKinds::Control(crate::lexer::ControlTokenKind::Eof),
                            index: 0,
                            len: 0,
                            location: TextLocation::new(0, 0),
                        })));
                    }
                }
                if cursor.idx >= tokens.len() {
                    return Ok(TokenCompare::IsNot(ParseError {
                        kind: ParseErrors::Eof,
                        location: tokens[cursor.idx - 1].location.clone(),
                        node: None,
                    }));
                }
                let mut current_token = &tokens[cursor.idx];
                while current_token.kind.is_whitespace() {
                    cursor.idx += 1;
                    current_token = &tokens[cursor.idx];
                }
                if *tok != current_token.kind {
                    return Ok(TokenCompare::IsNot(ParseError {
                        kind: ParseErrors::ExpectedToken {
                            expected: tok.clone(),
                            found: current_token.kind.clone(),
                        },
                        location: current_token.location.clone(),
                        node: None,
                    }));
                }
                Ok(TokenCompare::Is(Nodes::Token(current_token.clone())))
            }
            grammar::MatchToken::Node(node_name) => {
                match self.parse_node(grammar, lexer, node_name, cursor, globals, tokens, text) {
                    Ok(node) => return Ok(TokenCompare::Is(Nodes::Node(node))),
                    Err((err, node)) => match node.harderror {
                        true => return Err(err),
                        false => return Ok(TokenCompare::IsNot(err)),
                    },
                };
            }
            grammar::MatchToken::Word(word) => {
                let mut current_token = &tokens[cursor.idx];
                while current_token.kind.is_whitespace() {
                    cursor.idx += 1;
                    current_token = &tokens[cursor.idx];
                }
                if let TokenKinds::Text = current_token.kind {
                    if word != &lexer.stringify(&current_token, text) {
                        return Ok(TokenCompare::IsNot(ParseError {
                            kind: ParseErrors::ExpectedWord {
                                expected: word.to_string(),
                                found: current_token.kind.clone(),
                            },
                            location: current_token.location.clone(),
                            node: None,
                        }));
                    }
                } else {
                    return Ok(TokenCompare::IsNot(ParseError {
                        kind: ParseErrors::ExpectedWord {
                            expected: word.to_string(),
                            found: current_token.kind.clone(),
                        },
                        location: current_token.location.clone(),
                        node: None,
                    }));
                }
                Ok(TokenCompare::Is(Nodes::Token(current_token.clone())))
            }
            grammar::MatchToken::Enumerator(enumerator) => {
                #[cfg(feature = "debug")]
                println!(
                    "keys: {:?}",
                    grammar.enumerators.keys().collect::<Vec<&String>>()
                );
                #[cfg(feature = "debug")]
                println!("key: {enumerator}");
                #[cfg(feature = "debug")]
                println!("got: {}", grammar.enumerators.get(enumerator).is_some());
                let enumerator = match grammar.enumerators.get(enumerator) {
                    Some(enumerator) => enumerator,
                    None => {
                        return Err(ParseError {
                            kind: ParseErrors::EnumeratorNotFound(enumerator.clone()),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        });
                    }
                };
                let mut i = 0;
                let cursor_clone_local = cursor.clone();
                let token = loop {
                    if i >= enumerator.values.len() {
                        return Ok(TokenCompare::IsNot(ParseError {
                            kind: ParseErrors::ExpectedOneOf {
                                expected: enumerator.values.iter().map(|x| x.clone()).collect(),
                                found: tokens[cursor.idx].kind.clone(),
                            },
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        }));
                    }
                    let token = &enumerator.values[i];
                    match self.match_token(
                        grammar,
                        lexer,
                        token,
                        cursor,
                        globals,
                        cursor_clone,
                        tokens,
                        text,
                    )? {
                        TokenCompare::Is(val) => break val,
                        TokenCompare::IsNot(err) => {
                            *cursor = cursor_clone_local.clone();
                            if let Some(node) = &err.node {
                                if node.harderror {
                                    return Err(err);
                                }
                            }
                            i += 1;
                        }
                    }
                };
                #[cfg(feature = "debug")]
                println!("matched: {:?}", token);
                Ok(TokenCompare::Is(token))
            }
            grammar::MatchToken::Any => {
                let token = tokens[cursor.idx].clone();
                Ok(TokenCompare::Is(Nodes::Token(token)))
            }
        }
    }

    fn parse_parameters<'a>(
        &'a self,
        _grammar: &Grammar,
        _lexer: &Lexer,
        parameters: &Vec<grammar::Parameters>,
        cursor: &mut Cursor,
        globals: &mut Arena<VariableKind<'a>, GlobalVariableTag>,
        _cursor_clone: &Cursor,
        node: &mut Node<'a>,
        value: &Nodes<'a>,
        bus: &mut MsgBus,
        tokens: &Vec<Token>,
        _text: &str,
    ) -> Result<(), ParseError> {
        for parameter in parameters {
            match parameter {
                grammar::Parameters::Set(name) => {
                    let kind = match node.variables.get_mut(name) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Local(*name)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    match kind {
                        VariableKind::Node(ref mut single) => {
                            *single = Some(value.clone());
                        }
                        VariableKind::NodeList(list) => {
                            list.push(value.clone());
                        }
                        VariableKind::Boolean(_) => Err(ParseError {
                            kind: ParseErrors::CannotSetVariable(
                                VarKind::Local(*name),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Number(_) => Err(ParseError {
                            kind: ParseErrors::CannotSetVariable(
                                VarKind::Local(*name),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                    };
                }
                grammar::Parameters::Print(_str) => {
                    #[cfg(feature = "std")]
                    println!("{}", _str)
                }
                grammar::Parameters::Debug(variable) => match variable {
                    Some(_ident) => {
                        #[cfg(feature = "std")]
                        {
                            let kind = match node.variables.get(_ident) {
                                Some(kind) => kind,
                                None => {
                                    return Err(ParseError {
                                        kind: ParseErrors::VariableNotFound(VarKind::Local(
                                            *_ident,
                                        )),
                                        location: tokens[cursor.idx].location.clone(),
                                        node: None,
                                    })
                                }
                            };
                            println!("{:?}", kind);
                        }
                    }
                    None =>
                    {
                        #[cfg(feature = "std")]
                        if cursor.idx >= tokens.len() {
                            println!("Eof");
                        } else {
                            println!("{:?}", _lexer.stringify(&tokens[cursor.idx], _text));
                        }
                    }
                },
                grammar::Parameters::Increment(ident) => {
                    let kind = match node.variables.get_mut(ident) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Local(*ident)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    match kind {
                        VariableKind::Node(_) => Err(ParseError {
                            kind: ParseErrors::VariableNotFound(VarKind::Local(*ident)),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::NodeList(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*ident),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Boolean(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*ident),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Number(ref mut val) => {
                            *val += 1;
                        }
                    };
                }
                grammar::Parameters::Decrement(ident) => {
                    let kind = match node.variables.get_mut(ident) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Local(*ident)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    match kind {
                        VariableKind::Node(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*ident),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::NodeList(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*ident),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Boolean(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*ident),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Number(ref mut val) => {
                            *val -= 1;
                        }
                    };
                }
                grammar::Parameters::True(variable) => {
                    let kind = match node.variables.get_mut(variable) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Local(*variable)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    if let VariableKind::Boolean(ref mut val) = kind {
                        *val = true;
                    } else {
                        return Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        });
                    }
                }
                grammar::Parameters::False(variable) => {
                    let kind = match node.variables.get_mut(variable) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Local(*variable)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    if let VariableKind::Boolean(ref mut val) = kind {
                        *val = false;
                    } else {
                        return Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Local(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        });
                    }
                }
                grammar::Parameters::Global(variable) => {
                    let kind = match globals.get_mut(variable) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Global(*variable)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    match kind {
                        VariableKind::Node(single) => {
                            *single = Some(value.clone());
                        }
                        VariableKind::NodeList(list) => {
                            list.push(value.clone());
                        }
                        VariableKind::Boolean(_) => Err(ParseError {
                            kind: ParseErrors::CannotSetVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Number(_) => Err(ParseError {
                            kind: ParseErrors::CannotSetVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                    };
                }
                grammar::Parameters::IncrementGlobal(variable) => {
                    let kind = match globals.get_mut(variable) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Global(*variable)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    match kind {
                        VariableKind::Node(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::NodeList(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Boolean(_) => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        })?,
                        VariableKind::Number(val) => {
                            *val += 1;
                        }
                    };
                }
                grammar::Parameters::TrueGlobal(variable) => {
                    let kind = match globals.get_mut(variable) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Global(*variable)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    if let VariableKind::Boolean(val) = kind {
                        *val = true;
                    } else {
                        return Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        });
                    }
                }
                grammar::Parameters::FalseGlobal(variable) => {
                    let kind = match globals.get_mut(variable) {
                        Some(kind) => kind,
                        None => {
                            return Err(ParseError {
                                kind: ParseErrors::VariableNotFound(VarKind::Global(*variable)),
                                location: tokens[cursor.idx].location.clone(),
                                node: None,
                            })
                        }
                    };
                    if let VariableKind::Boolean(val) = kind {
                        *val = false;
                    } else {
                        return Err(ParseError {
                            kind: ParseErrors::UncountableVariable(
                                VarKind::Global(*variable),
                                kind.clone(),
                            ),
                            location: tokens[cursor.idx].location.clone(),
                            node: None,
                        });
                    }
                }
                grammar::Parameters::HardError(value) => {
                    node.harderror = *value;
                }
                grammar::Parameters::NodeStart => {
                    node.first_string_idx = tokens[cursor.idx].index;
                }
                grammar::Parameters::NodeEnd => {
                    node.last_string_idx = tokens[cursor.idx].index + tokens[cursor.idx].len;
                }
                grammar::Parameters::Back(steps) => {
                    bus.send(Msg::Back(*steps as usize));
                }
                grammar::Parameters::Return => {
                    bus.send(Msg::Return);
                }
                grammar::Parameters::Goto(label) => {
                    bus.send(Msg::Goto(label.to_string()));
                }
                grammar::Parameters::Break(n) => {
                    bus.send(Msg::Break(*n));
                }
            }
        }
        Ok(())
    }
}

enum TokenCompare<'a> {
    Is(Nodes<'a>),
    IsNot(ParseError<'a>),
}

#[derive(Debug)]
pub struct ParseResult<'a> {
    pub entry: Node<'a>,
    pub globals: Arena<VariableKind<'a>, GlobalVariableTag>,
}

pub mod map_tools {
    use super::*;

    pub fn try_get_node<'a>(map: &'a Map<String, VariableKind>, key: &str) -> Option<&'a Node<'a>> {
        match map.get(key) {
            Some(VariableKind::Node(Some(Nodes::Node(node)))) => Some(node),
            _ => None,
        }
    }

    pub fn get_node<'a>(map: &'a Map<String, VariableKind>, key: &str) -> &'a Node<'a> {
        match map.get(key) {
            Some(n) => match n {
                VariableKind::Node(Some(Nodes::Node(node))) => node,
                _ => panic!("Node found with a different type {:#?}", n),
            },
            _ => panic!("Node not found"),
        }
    }

    pub fn try_get_node_list<'a>(
        map: &'a Map<String, VariableKind>,
        key: &str,
    ) -> Option<&'a Vec<Nodes<'a>>> {
        match map.get(key) {
            Some(VariableKind::NodeList(list)) => Some(list),
            _ => None,
        }
    }

    pub fn get_node_list<'a>(map: &'a Map<String, VariableKind>, key: &str) -> &'a Vec<Nodes<'a>> {
        match map.get(key) {
            Some(list) => match list {
                VariableKind::NodeList(list) => list,
                _ => panic!("Node list found with a different type {:#?}", list),
            },
            _ => panic!("Node list not found"),
        }
    }

    pub fn try_get_boolean(map: &Map<String, VariableKind>, key: &str) -> Option<bool> {
        match map.get(key) {
            Some(VariableKind::Boolean(val)) => Some(*val),
            _ => None,
        }
    }

    pub fn get_boolean(map: &Map<String, VariableKind>, key: &str) -> bool {
        match map.get(key) {
            Some(val) => match val {
                VariableKind::Boolean(val) => *val,
                _ => panic!("Boolean found with a different type {:#?}", val),
            },
            _ => panic!("Boolean not found"),
        }
    }

    pub fn try_get_number(map: &Map<String, VariableKind>, key: &str) -> Option<i32> {
        match map.get(key) {
            Some(VariableKind::Number(val)) => Some(*val),
            _ => None,
        }
    }

    pub fn get_number(map: &Map<String, VariableKind>, key: &str) -> i32 {
        match map.get(key) {
            Some(val) => match val {
                VariableKind::Number(val) => *val,
                _ => panic!("Number found with a different type {:#?}", val),
            },
            _ => panic!("Number not found"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum Nodes<'a> {
    Node(Node<'a>),
    Token(Token),
}

impl<'a> Nodes<'a> {
    pub fn is_node(&self) -> bool {
        match self {
            Nodes::Node(_) => true,
            _ => false,
        }
    }

    pub fn is_token(&self) -> bool {
        match self {
            Nodes::Token(_) => true,
            _ => false,
        }
    }

    pub fn unwrap_node(&self) -> &Node {
        match self {
            Nodes::Node(node) => node,
            _ => panic!("unwrap_node called on {:#?}", self),
        }
    }

    pub fn unwrap_token(&self) -> &Token {
        match self {
            Nodes::Token(token) => token,
            _ => panic!("unwrap_token called on {:#?}", self),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node<'a> {
    pub name: &'a str,
    pub variables: Arena<VariableKind<'a>, VariableTag>,
    pub(crate) first_string_idx: usize,
    pub(crate) last_string_idx: usize,
    pub(crate) harderror: bool,
    pub docs: Option<&'a str>,
    pub location: TextLocation,
}

impl<'a> Node<'a> {
    pub fn new(name: &'a str) -> Node<'a> {
        Node {
            name,
            variables: Arena::new(),
            first_string_idx: 0,
            last_string_idx: 0,
            harderror: false,
            docs: None,
            location: TextLocation::new(0, 0),
        }
    }

    pub fn from_grammar(
        grammar: &'a Grammar<'a>,
        name: &Key<NodeTag>,
    ) -> Result<Node<'a>, ParseError<'a>> {
        let found = match grammar.nodes.get(name) {
            Some(node) => node,
            None => {
                return Err(ParseError {
                    kind: ParseErrors::NodeNotFound(*name),
                    location: TextLocation::new(0, 0),
                    node: None,
                })
            }
        };
        let mut node = Node::new(found.name.clone());
        node.variables = Self::variables_from_grammar(&found.variables)?;
        node.docs = found.docs.clone();
        Ok(node)
    }

    pub fn variables_from_grammar<Tag>(
        variables: &Arena<grammar::VariableKind, Tag>,
    ) -> Result<Arena<VariableKind, Tag>, ParseError> {
        let mut result = Arena::new();
        for value in variables.iter() {
            let var = match value {
                crate::grammar::VariableKind::Node => VariableKind::Node(None),
                crate::grammar::VariableKind::NodeList => VariableKind::NodeList(Vec::new()),
                crate::grammar::VariableKind::Boolean => VariableKind::Boolean(false),
                crate::grammar::VariableKind::Number => VariableKind::Number(0),
            };
            result.push(var);
        }
        Ok(result)
    }
}

fn err<'a>(
    error: ParseErrors<'a>,
    cursor: &mut Cursor,
    cursor_clone: &Cursor,
    location: &TextLocation,
    node: Option<Node<'a>>,
) -> Result<(), ParseError<'a>> {
    *cursor = cursor_clone.clone();
    Err(ParseError {
        kind: error,
        location: location.clone(),
        node,
    })
}

#[derive(Debug, Clone)]
pub enum VariableKind<'a> {
    Node(Option<Nodes<'a>>),
    NodeList(Vec<Nodes<'a>>),
    Boolean(bool),
    Number(i32),
}

impl<'a> VariableKind<'a> {
    pub fn is_node(&self) -> bool {
        match self {
            VariableKind::Node(_) => true,
            _ => false,
        }
    }

    pub fn is_node_list(&self) -> bool {
        match self {
            VariableKind::NodeList(_) => true,
            _ => false,
        }
    }

    pub fn is_boolean(&self) -> bool {
        match self {
            VariableKind::Boolean(_) => true,
            _ => false,
        }
    }

    pub fn is_number(&self) -> bool {
        match self {
            VariableKind::Number(_) => true,
            _ => false,
        }
    }

    pub fn unwrap_node(&self) -> &Nodes {
        match self {
            VariableKind::Node(node) => node.as_ref().unwrap(),
            _ => panic!("unwrap_node called on {:#?}", self),
        }
    }

    pub fn unwrap_node_list(&self) -> &Vec<Nodes> {
        match self {
            VariableKind::NodeList(list) => list,
            _ => panic!("unwrap_node_list called on {:#?}", self),
        }
    }

    pub fn unwrap_boolean(&self) -> &bool {
        match self {
            VariableKind::Boolean(val) => val,
            _ => panic!("unwrap_boolean called on {:#?}", self),
        }
    }

    pub fn unwrap_number(&self) -> &i32 {
        match self {
            VariableKind::Number(val) => val,
            _ => panic!("unwrap_number called on {:#?}", self),
        }
    }
}

#[derive(Clone)]
pub struct ParseError<'a> {
    kind: ParseErrors<'a>,
    location: TextLocation,
    node: Option<Node<'a>>,
}

impl<'a> fmt::Debug for ParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} at {:?}", self.kind, self.location)?;
        match &self.node {
            Some(node) => {
                let mut txt = format!("\nError in node: {:?}", node.name);
                if let Some(docs) = &node.docs {
                    txt.push_str(&format!("\n{}", docs));
                }
                write!(f, "{}\n", txt)
            }
            None => Ok(()),
        }
    }
}

impl<'a> fmt::Display for ParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} at {:?}", self.kind, self.location)?;
        match &self.node {
            Some(node) => {
                let mut txt = format!("\nError in node: {:?}", node.name);
                if let Some(docs) = &node.docs {
                    txt.push_str(&format!("\n{}", docs));
                }
                write!(f, "{}\n", txt)
            }
            None => Ok(()),
        }
    }
}

#[derive(Clone)]
pub enum ParseErrors<'a> {
    /// Parser not fully implemented - My fault
    ParserNotFullyImplemented,
    /// Node not found - Developer error
    NodeNotFound(Key<NodeTag>),
    /// Expected a token, found a token
    ExpectedToken {
        expected: TokenKinds,
        found: TokenKinds,
    },
    /// Expected a word, found a token
    ExpectedWord {
        expected: String,
        found: TokenKinds,
    },
    /// Enumerator not found - Developer error
    EnumeratorNotFound(Key<EnumeratorTag>),
    /// Expected to not be
    ExpectedToNotBe(TokenKinds),
    /// Variable not found - Developer error
    VariableNotFound(VarKind),
    /// Uncountable variable - Developer error
    UncountableVariable(VarKind, VariableKind<'a>),
    /// Cannot set variable - Developer error
    CannotSetVariable(VarKind, VariableKind<'a>),
    /// Custom error message
    Message(&'a str),
    /// Unexpected end of file
    Eof,
    /// Label not found - Developer error
    LabelNotFound(String),
    /// Cannot go back - Developer error
    CannotGoBack(usize),
    /// Cannot break - Developer error
    CannotBreak(usize),
    /// Expected one of
    ExpectedOneOf {
        expected: Vec<MatchToken<'a>>,
        found: TokenKinds,
    },
    /// Could not find token
    CouldNotFindToken(MatchToken<'a>),
    /// This error occurers when the parser ends on different token than eof
    ///
    /// This behaviour can be changed by setting the `eof` field in the grammar
    MissingEof(TokenKinds),
    MissingEntry,

    /// Control key
    Ok,
}

impl<'a> fmt::Debug for ParseErrors<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseErrors::ParserNotFullyImplemented => write!(f, "Parser not fully implemented"),
            ParseErrors::NodeNotFound(name) => write!(f, "Node not found: working on it :)"),
            ParseErrors::ExpectedToken { expected, found } => {
                write!(f, "Expected token {:?}, found {:?}", expected, found)
            }
            ParseErrors::ExpectedWord { expected, found } => {
                write!(f, "Expected word {}, found {:?}", expected, found)
            }
            ParseErrors::EnumeratorNotFound(name) => {
                write!(f, "Enumerator not found: working on it :)")
            }
            ParseErrors::ExpectedToNotBe(kind) => write!(f, "Expected to not be {:?}", kind),
            ParseErrors::VariableNotFound(name) => {
                write!(f, "Variable not found: working on it :)")
            }
            ParseErrors::UncountableVariable(name, kind) => {
                write!(f, "Uncountable variable: dont know<{:?}>", kind)
            }
            ParseErrors::CannotSetVariable(name, kind) => {
                write!(f, "Cannot set variable: dont know<{:?}>", kind)
            }
            ParseErrors::Message(message) => write!(f, "{}", message),
            ParseErrors::Eof => write!(f, "Unexpected end of file"),
            ParseErrors::LabelNotFound(name) => write!(f, "Label not found: {}", name),
            ParseErrors::CannotGoBack(steps) => write!(f, "Cannot go back {} steps", steps),
            ParseErrors::CannotBreak(n) => write!(f, "Cannot break {} more steps", n),
            ParseErrors::ExpectedOneOf { expected, found } => {
                write!(f, "Expected one of {:?}, found {:?}", expected, found)
            }
            ParseErrors::CouldNotFindToken(kind) => write!(f, "Could not find token {:?}", kind),
            ParseErrors::Ok => write!(f, "If you see this, it could be a bug in the parser"),
            ParseErrors::MissingEof(found) => write!(
                f,
                "Could not parse to the end of the file - found {:?}",
                found
            ),
            ParseErrors::MissingEntry => write!(f, "Entry node not set"),
        }
    }
}

/// A cursor is used to keep track of the current position in the token stream and other useful information (no useful information yet)
#[derive(Clone, Debug)]
struct Cursor {
    /// Current index in the token stream
    idx: usize,
    /// Whether to advance the cursor or not
    ///
    /// This is used to prevent the cursor from advancing more than once in a single iteration
    /// This could happen if a rule is executed and the cursor is advanced, then the rule returns and the cursor is advanced again
    to_advance: bool,
}

struct MsgBus {
    messages: Vec<Msg>,
}

impl MsgBus {
    fn new() -> MsgBus {
        MsgBus {
            messages: Vec::new(),
        }
    }

    fn send(&mut self, msg: Msg) {
        self.messages.push(msg);
    }

    fn receive(&mut self) -> Option<Msg> {
        self.messages.pop()
    }
}

enum Msg {
    Return,
    Break(usize),
    Goto(String),
    Back(usize),
    Ok,
}

impl Msg {
    fn push(self, bus: &mut MsgBus) {
        bus.send(self);
    }
}
