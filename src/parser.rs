use crate::{
    grammar::{ErrorDefinition, Parameters, VarKind},
    Map,
};

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
pub struct Parser<'a> {
    pub entry: Option<&'a str>,
    /// Option to enable error on eof
    pub eof_error: bool,
}

impl<'a> Default for Parser<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Parser<'a> {
    pub fn new() -> Parser<'a> {
        Parser {
            entry: None,
            eof_error: false,
        }
    }

    pub(crate) fn parse(
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
                    location: TextLocation::new(0, 0, 0, 0),
                    node: None,
                    hint: Some("Set an entry point in the parser"),
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
                    if cursor.to_advance && cursor.idx < tokens.len() - 1 {
                        cursor.to_advance = false;
                        cursor.idx += 1;
                    }
                    // If the grammar has an eof token, we need to check if the cursor is at the end of the tokens
                    // Consume all the whitespace tokens
                    while cursor.idx < tokens.len() - 1 && tokens[cursor.idx].kind.is_whitespace() {
                        cursor.idx += 1;
                    }
                    if let TokenKinds::Control(crate::lexer::ControlTokenKind::Eof) =
                        tokens[cursor.idx].kind
                    {
                        node
                    } else {
                        return Err(ParseError {
                            kind: ParseErrors::MissingEof(tokens[cursor.idx].kind.clone()),
                            location: tokens[cursor.idx].location,
                            node: Some(node),
                            hint: Some("Remove all unneccesary text from the end of file"),
                        });
                    }
                }
            }
            Err(err) => return Err(err.1),
        };

        Ok(ParseResult { entry, globals })
    }

    fn parse_node(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        name: &'a str,
        cursor: &mut Cursor,
        globals: &mut Map<String, VariableKind<'a>>,
        tokens: &Vec<Token>,
        text: &'a str,
    ) -> Result<Node<'a>, (bool, ParseError<'a>)> {
        #[cfg(feature = "debug")]
        println!("-- start, cursor: {:?}", cursor);
        let mut node = match Node::from_grammar(grammar, name) {
            Ok(node) => node,
            Err(err) => return Err((false, err)),
        };
        node.first_string_idx = tokens[cursor.idx].index;
        // In case the node fails to parse, we want to restore the cursor to its original position
        let cursor_clone = cursor.clone();
        let rules = match grammar.nodes.get(name) {
            Some(node) => &node.rules,
            None => {
                return Err((
                    node.commit,
                    ParseError {
                        kind: ParseErrors::NodeNotFound(name),
                        location: tokens[cursor.idx].location,
                        node: Some(node.clone()),
                        hint: Some("Please run the parser through validator with .success()"),
                    },
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
        println!("-- end: {}, cursor: {:?}", node.name, cursor);

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
                    node.commit,
                    ParseError {
                        kind: ParseErrors::CannotBreak(*n),
                        location: tokens[cursor.idx].location,
                        node: Some(node.clone()),
                        hint: None,
                    },
                )),
                Msg::Back(steps) => Err((
                    node.commit,
                    ParseError {
                        kind: ParseErrors::CannotGoBack(*steps),
                        location: tokens[cursor.idx].location,
                        node: Some(node.clone()),
                        hint: None,
                    },
                )),
                Msg::Goto(label) => Err((
                    node.commit,
                    ParseError {
                        kind: ParseErrors::LabelNotFound(label.to_string()),
                        location: tokens[cursor.idx].location,
                        node: Some(node.clone()),
                        hint: None,
                    },
                )),
            },
            Err(mut err) => {
                #[cfg(feature = "debug")]
                println!("error: {:?}", err);
                *cursor = cursor_clone;
                if err.node.is_none() {
                    err.node = Some(node.clone());
                }
                Err((node.commit, err))
            }
        }
    }

    fn parse_rules(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        rules: &'a Vec<grammar::Rule<'a>>,
        cursor: &mut Cursor,
        globals: &mut Map<String, VariableKind<'a>>,
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
                            location: tokens[cursor.idx - 1].location,
                            node: Some(node.clone()),
                            hint: None,
                        });
                    } else {
                        cursor.idx -= 1;
                    }
                }
            }
            #[cfg(feature = "debug")]
            println!(
                "tok: <{}> kind: {:?} -- parent: {}",
                &tokens[cursor.idx].stringify(text),
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
                        Some(parameters),
                        text,
                    )? {
                        TokenCompare::Is(val) => {
                            let is_token = val.is_token();
                            self.parse_parameters(
                                parameters,
                                cursor,
                                globals,
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
                        None,
                        text,
                    )? {
                        TokenCompare::Is(_) => {
                            err(
                                ParseErrors::ExpectedToNotBe(tokens[cursor.idx].kind.clone()),
                                cursor,
                                cursor_clone,
                                &tokens[cursor.idx].location,
                                Some(node.clone()),
                                Some(&parameters),
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
                grammar::Rule::IsOneOf {
                    tokens: pos_tokens,
                    parameters,
                } => {
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
                            token,
                            cursor,
                            globals,
                            cursor_clone,
                            tokens,
                            Some(parameters),
                            text,
                        )? {
                            Is(val) => {
                                #[cfg(feature = "debug")]
                                println!("success");
                                found = true;
                                let is_token = val.is_token();
                                self.parse_parameters(
                                    parameters,
                                    cursor,
                                    globals,
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
                                    if node.commit {
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
                            Some(&parameters),
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
                        Some(parameters),
                        text,
                    )? {
                        Is(val) => {
                            let is_token = val.is_token();
                            self.parse_parameters(
                                parameters,
                                cursor,
                                globals,
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
                            if let Some(ref node) = err.node {
                                if node.commit {
                                    return Err(err);
                                }
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
                            token,
                            cursor,
                            globals,
                            cursor_clone,
                            tokens,
                            Some(parameters),
                            text,
                        )? {
                            Is(val) => {
                                found = true;
                                let is_token = val.is_token();
                                self.parse_parameters(
                                    parameters,
                                    cursor,
                                    globals,
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
                            IsNot(err) => {
                                if let Some(ref node) = err.node {
                                    if node.commit {
                                        return Err(err);
                                    }
                                }
                            }
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
                        Some(parameters),
                        text,
                    )? {
                        TokenCompare::Is(val) => {
                            let is_token = val.is_token();
                            self.parse_parameters(
                                parameters,
                                cursor,
                                globals,
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
                        TokenCompare::IsNot(err) => {
                            if let Some(ref node) = err.node {
                                if node.commit {
                                    return Err(err);
                                }
                            }
                        }
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
                        Some(parameters),
                        text,
                    )? {
                        // No need to handle the error here
                        cursor.idx += 1;
                        if cursor.idx >= tokens.len() {
                            return Err(ParseError {
                                kind: ParseErrors::CouldNotFindToken(token.clone()),
                                location: tokens[cursor.idx - 1].location,
                                node: Some(node.clone()),
                                hint: None,
                            });
                        }
                    }
                    self.parse_parameters(
                        parameters,
                        cursor,
                        globals,
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
                        let left = left.get(&node.variables, globals).unwrap();
                        let right = right.get(&node.variables, globals).unwrap();

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
                    grammar::Commands::Error { err } => Err(ParseError {
                        kind: ParseErrors::Message(err),
                        location: tokens[cursor.idx].location,
                        node: Some(node.clone()),
                        hint: None,
                    })?,
                    grammar::Commands::Commit { set } => {
                        node.commit = *set;
                    }
                    grammar::Commands::Goto { label } => {
                        msg_bus.send(Msg::Goto(label.to_string()));
                    }
                    grammar::Commands::Label { name: _ } => (),
                    grammar::Commands::Print { message: _msg } => {
                        #[cfg(feature = "std")]
                        println!("{}", _msg)
                    }
                    grammar::Commands::Return => {
                        msg_bus.send(Msg::Return);
                    }
                    grammar::Commands::Start => node.first_string_idx = tokens[cursor.idx].index,
                    grammar::Commands::End => {
                        node.last_string_idx = tokens[cursor.idx].index + tokens[cursor.idx].len
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
                                Some(parameters),
                                text,
                            )? {
                                Is(val) => {
                                    found = true;
                                    let is_token = val.is_token();
                                    self.parse_parameters(
                                        parameters,
                                        cursor,
                                        globals,
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
                                IsNot(err) => {
                                    if let Some(ref node) = err.node {
                                        if node.commit {
                                            return Err(err);
                                        }
                                    }
                                }
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
                            None,
                        )?;
                    }
                }
                grammar::Rule::Debug { target } => {
                    #[cfg(feature = "std")]
                    {
                        match target {
                            Some(_ident) => {
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
                                    println!("{:?}", tokens[cursor.idx].stringify(text));
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
                            if let grammar::Rule::Command {
                                command: grammar::Commands::Label { name },
                            } = &rules[j]
                            {
                                if *name == label {
                                    i = j;
                                    break;
                                }
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
            if cursor.to_advance {
                cursor.to_advance = false;
                cursor.idx += 1;
                if cursor.idx >= tokens.len() {
                    if self.eof_error {
                        return Err(ParseError {
                            kind: ParseErrors::Eof,
                            location: tokens[cursor.idx - 1].location,
                            node: Some(node.clone()),
                            hint: None,
                        });
                    } else {
                        cursor.idx -= 1;
                    }
                }
            }
        }
        Ok(Msg::Ok)
    }

    fn find_hint<'b>(parameters: Option<&'b [grammar::Parameters<'b>]>) -> Option<&'b str> {
        parameters?.iter().find_map(|p| {
            if let grammar::Parameters::Hint(s) = p {
                Some(*s)
            } else {
                None
            }
        })
    }

    fn match_token(
        &'a self,
        grammar: &'a Grammar<'a>,
        lexer: &Lexer,
        token: &'a grammar::MatchToken,
        cursor: &mut Cursor,
        globals: &mut Map<String, VariableKind<'a>>,
        cursor_clone: &Cursor,
        tokens: &Vec<Token>,
        parameters: Option<&'a [Parameters<'a>]>,
        text: &'a str,
    ) -> Result<TokenCompare<'a>, ParseError<'a>> {
        match token {
            grammar::MatchToken::Token(tok) => {
                // if tok.is_whitespace() {
                //     let current = &tokens[cursor.idx];
                //     while cursor.idx > 0 && tokens[cursor.idx - 1].kind.is_whitespace() {
                //         todo!("need to correctly handle matching whitespace")
                //     }
                // }
                if *tok == TokenKinds::Control(crate::lexer::ControlTokenKind::Eof)
                    && cursor.idx >= tokens.len()
                {
                    return Ok(TokenCompare::Is(Nodes::Token(Token {
                        kind: TokenKinds::Control(crate::lexer::ControlTokenKind::Eof),
                        index: 0,
                        len: 0,
                        location: TextLocation::new(0, 0, 0, 0),
                    })));
                }
                if cursor.idx >= tokens.len() {
                    return Ok(TokenCompare::IsNot(ParseError {
                        kind: ParseErrors::Eof,
                        location: tokens[cursor.idx - 1].location,
                        node: None,
                        hint: Self::find_hint(parameters),
                    }));
                }
                let mut current_token = &tokens[cursor.idx];
                let mut peek = 0;
                while current_token.kind.is_whitespace() {
                    if *tok == current_token.kind {
                        cursor.idx += peek;
                        return Ok(TokenCompare::Is(Nodes::Token(current_token.clone())));
                    }
                    peek += 1;
                    current_token = &tokens[cursor.idx + peek];
                }
                if *tok != current_token.kind {
                    return Ok(TokenCompare::IsNot(ParseError {
                        kind: ParseErrors::ExpectedToken {
                            expected: tok.clone(),
                            found: current_token.kind.clone(),
                        },
                        location: current_token.location,
                        node: None,
                        hint: Self::find_hint(parameters),
                        // hint,
                    }));
                }
                cursor.idx += peek;
                Ok(TokenCompare::Is(Nodes::Token(current_token.clone())))
            }
            grammar::MatchToken::Node(node_name) => {
                match self.parse_node(grammar, lexer, node_name, cursor, globals, tokens, text) {
                    Ok(node) => Ok(TokenCompare::Is(Nodes::Node(node))),
                    Err((commit, err)) => match commit {
                        true => Err(err),
                        false => Ok(TokenCompare::IsNot(err)),
                    },
                }
            }
            grammar::MatchToken::Word(word) => {
                let mut current_token = &tokens[cursor.idx];
                let mut peek = 0;
                while current_token.kind.is_whitespace() {
                    peek += 1;
                    current_token = &tokens[cursor.idx + peek];
                }
                if !matches!(current_token.kind, TokenKinds::Text)
                    || word != &current_token.stringify(text)
                {
                    if word != &current_token.stringify(text) {
                        return Ok(TokenCompare::IsNot(ParseError {
                            kind: ParseErrors::ExpectedWord {
                                expected: word.to_string(),
                                found: current_token.kind.clone(),
                            },
                            location: current_token.location,
                            node: None,
                            hint: Self::find_hint(parameters),
                        }));
                    }
                }
                cursor.idx += peek;
                Ok(TokenCompare::Is(Nodes::Token(current_token.clone())))
            }
            grammar::MatchToken::Enumerator(enumerator) => {
                let enumerator = match grammar.enumerators.get(*enumerator) {
                    Some(enumerator) => enumerator,
                    None => {
                        return Err(ParseError {
                            kind: ParseErrors::EnumeratorNotFound(enumerator),
                            location: tokens[cursor.idx].location,
                            node: None,
                            hint: Self::find_hint(parameters),
                        });
                    }
                };
                let mut i = 0;
                let cursor_clone_local = cursor.clone();
                let token = loop {
                    if i >= enumerator.values.len() {
                        return Ok(TokenCompare::IsNot(ParseError {
                            kind: ParseErrors::ExpectedOneOf {
                                expected: enumerator.values.to_vec(),
                                found: tokens[cursor.idx].kind.clone(),
                            },
                            location: tokens[cursor.idx].location,
                            node: None,
                            hint: Self::find_hint(parameters),
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
                        parameters,
                        text,
                    )? {
                        TokenCompare::Is(val) => break val,
                        TokenCompare::IsNot(err) => {
                            *cursor = cursor_clone_local.clone();
                            if let Some(node) = &err.node {
                                if node.commit {
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

    fn parse_parameters(
        &'a self,
        parameters: &'a Vec<grammar::Parameters>,
        cursor: &mut Cursor,
        globals: &mut Map<String, VariableKind<'a>>,
        node: &mut Node<'a>,
        value: &Nodes<'a>,
        bus: &mut MsgBus,
        tokens: &Vec<Token>,
        text: &str,
    ) -> Result<(), ParseError<'a>> {
        for parameter in parameters {
            match parameter {
                grammar::Parameters::Set(name) => {
                    let kind = name
                        .get_mut(&mut node.variables, globals)
                        .expect("Variable exists not :(");
                    match kind {
                        VariableKind::Node(ref mut single) => {
                            *single = Some(value.clone());
                        }
                        VariableKind::NodeList(list) => {
                            list.push(value.clone());
                        }
                        _ => Err(ParseError {
                            kind: ParseErrors::CannotSetVariable(*name, kind.clone()),
                            location: tokens[cursor.idx].location,
                            node: None,
                            hint: None,
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
                            let kind = _ident.get(&node.variables, globals);
                            println!("{:?}", kind.map(|k| k.stringify(text)));
                        }
                    }
                    None =>
                    {
                        #[cfg(feature = "std")]
                        if cursor.idx >= tokens.len() {
                            println!("Eof");
                        } else {
                            println!("{:?}", tokens[cursor.idx].stringify(text));
                        }
                    }
                },
                grammar::Parameters::Increment(ident) => {
                    let kind = ident.get_mut(&mut node.variables, globals).unwrap();
                    match kind {
                        VariableKind::Number(ref mut val) => {
                            *val += 1;
                        }
                        _ => Err(ParseError {
                            kind: ParseErrors::UncountableVariable(*ident, kind.clone()),
                            location: tokens[cursor.idx].location,
                            node: None,
                            hint: None,
                        })?,
                    };
                }
                grammar::Parameters::Decrement(ident) => {
                    let kind = ident.get_mut(&mut node.variables, globals).unwrap();
                    match kind {
                        VariableKind::Number(ref mut val) => {
                            *val -= 1;
                        }
                        _ => Err(ParseError {
                            hint: None,
                            kind: ParseErrors::UncountableVariable(*ident, kind.clone()),
                            location: tokens[cursor.idx].location,
                            node: None,
                        })?,
                    };
                }
                grammar::Parameters::True(variable) => {
                    let kind = variable.get_mut(&mut node.variables, globals).unwrap();
                    if let VariableKind::Boolean(ref mut val) = kind {
                        *val = true;
                    } else {
                        return Err(ParseError {
                            hint: None,
                            kind: ParseErrors::UncountableVariable(*variable, kind.clone()),
                            location: tokens[cursor.idx].location,
                            node: None,
                        });
                    }
                }
                grammar::Parameters::False(variable) => {
                    let kind = variable.get_mut(&mut node.variables, globals).unwrap();
                    if let VariableKind::Boolean(ref mut val) = kind {
                        *val = false;
                    } else {
                        return Err(ParseError {
                            hint: None,
                            kind: ParseErrors::UncountableVariable(*variable, kind.clone()),
                            location: tokens[cursor.idx].location,
                            node: None,
                        });
                    }
                }
                grammar::Parameters::CloneValue(var1, var2) => {
                    var2.set(var1, &mut node.variables, globals);
                }
                grammar::Parameters::Commit(value) => {
                    node.commit = *value;
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
                grammar::Parameters::Hint(_) => (),
                grammar::Parameters::Fail(msg) => {
                    return Err(ParseError {
                        kind: ParseErrors::Message(&msg),
                        location: tokens[cursor.idx].location,
                        node: None,
                        hint: Self::find_hint(Some(parameters)),
                    })
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
    pub globals: Map<String, VariableKind<'a>>,
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

    pub fn unwrap_node(&self) -> &Node<'_> {
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
    pub variables: Map<String, VariableKind<'a>>,
    pub(crate) first_string_idx: usize,
    pub(crate) last_string_idx: usize,
    pub(crate) commit: bool,
    pub docs: Option<&'a str>,
    pub location: TextLocation,
}

impl<'a> Node<'a> {
    pub fn new(name: &'a str) -> Node<'a> {
        Node {
            name,
            variables: Map::new(),
            first_string_idx: 0,
            last_string_idx: 0,
            commit: false,
            docs: None,
            location: TextLocation::new(0, 0, 0, 0),
        }
    }

    pub fn from_grammar(
        grammar: &'a Grammar<'a>,
        name: &'a str,
    ) -> Result<Node<'a>, ParseError<'a>> {
        let found = match grammar.nodes.get(name) {
            Some(node) => node,
            None => {
                return Err(ParseError {
                    hint: None,
                    kind: ParseErrors::NodeNotFound(name),
                    location: TextLocation::new(0, 0, 0, 0),
                    node: None,
                })
            }
        };
        let mut node = Node::new(found.name);
        node.variables = Self::variables_from_grammar(&found.variables)?;
        node.docs = found.docs;
        Ok(node)
    }

    pub fn variables_from_grammar(
        variables: &[(&'a str, grammar::VariableKind)],
    ) -> Result<Map<String, VariableKind<'a>>, ParseError<'a>> {
        let mut result = Map::new();
        for value in variables.iter() {
            let var = match value.1 {
                crate::grammar::VariableKind::Node => VariableKind::Node(None),
                crate::grammar::VariableKind::NodeList => VariableKind::NodeList(Vec::new()),
                crate::grammar::VariableKind::Boolean => VariableKind::Boolean(false),
                crate::grammar::VariableKind::Number => VariableKind::Number(0),
            };
            result.insert(value.0.to_string(), var);
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
    parameters: Option<&'a [Parameters<'a>]>,
) -> Result<(), ParseError<'a>> {
    *cursor = cursor_clone.clone();
    Err(ParseError {
        kind: error,
        location: *location,
        node,
        hint: Parser::find_hint(parameters),
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

    pub fn unwrap_node(&self) -> &Nodes<'_> {
        match self {
            VariableKind::Node(Some(node)) => node,
            _ => panic!("unwrap_node called on {:#?}", self),
        }
    }

    pub fn try_unwrap_node(&self) -> &Option<Nodes<'_>> {
        match self {
            VariableKind::Node(n) => n,
            _ => panic!("try_unwrap_node called on {self:#?}"),
        }
    }

    pub fn unwrap_node_list(&self) -> &Vec<Nodes<'_>> {
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

    pub fn stringify(&self, text: &'a str) -> String {
        match self {
            VariableKind::Node(Some(nodes)) => nodes.stringify(text).to_string(),
            VariableKind::NodeList(items) => format!("Nodes len: {}", items.len()),
            VariableKind::Boolean(v) => v.to_string(),
            VariableKind::Number(v) => v.to_string(),
            VariableKind::Node(None) => String::from("None"),
        }
    }
}

#[derive(Clone)]
pub struct ParseError<'a> {
    pub kind: ParseErrors<'a>,
    pub location: TextLocation,
    pub node: Option<Node<'a>>,
    pub hint: Option<&'a str>,
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
                writeln!(f, "{}", txt)
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
                writeln!(f, "{}", txt)
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
    NodeNotFound(&'a str),
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
    EnumeratorNotFound(&'a str),
    /// Expected to not be
    ExpectedToNotBe(TokenKinds),
    /// Variable not found - Developer error
    VariableNotFound(VarKind<'a>),
    /// Uncountable variable - Developer error
    UncountableVariable(VarKind<'a>, VariableKind<'a>),
    /// Cannot set variable - Developer error
    CannotSetVariable(VarKind<'a>, VariableKind<'a>),
    /// Custom error message
    Message(&'a ErrorDefinition),
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

impl<'a> ParseErrors<'a> {
    pub fn id_and_header(&self) -> (&'static str, &'static str) {
        match self {
            ParseErrors::ParserNotFullyImplemented => ("200", "Parser not fully implemented"),
            ParseErrors::NodeNotFound(_) => ("150", "Node not found"),
            ParseErrors::ExpectedToken { .. } => ("201", "Unexpected token"),
            ParseErrors::ExpectedWord { .. } => ("201", "Unexpected token"),
            ParseErrors::ExpectedToNotBe(_) => ("201", "Unexpected token"),
            ParseErrors::EnumeratorNotFound(_) => ("151", "Enumerator not found"),
            ParseErrors::VariableNotFound(_) => ("152", "Variable not found"),
            ParseErrors::UncountableVariable(_, _) => ("153", "Variable is uncountable"),
            ParseErrors::CannotSetVariable(_, _) => ("154", "Variable can not be set"),
            ParseErrors::Message(def) => (def.code, def.header),
            ParseErrors::Eof => ("202", "Unexpected end of file"),
            ParseErrors::LabelNotFound(_) => ("155", "Label bot found"),
            ParseErrors::CannotGoBack(_) => ("156", "Can not go back"),
            ParseErrors::CannotBreak(_) => ("157", "Can not break"),
            ParseErrors::ExpectedOneOf { .. } => ("201", "Unexpected token"),
            ParseErrors::CouldNotFindToken(_) => ("158", "Can not find token"),
            ParseErrors::MissingEof(_) => ("203", "Could not parse until the end"),
            ParseErrors::MissingEntry => ("159", "Missing entry point"),
            ParseErrors::Ok => ("---", "Ok"),
        }
    }
}

impl<'a> fmt::Debug for ParseErrors<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ParseErrors::ParserNotFullyImplemented => write!(f, "Parser not fully implemented"),
            ParseErrors::NodeNotFound(_name) => write!(f, "Node not found: working on it :)"),
            ParseErrors::ExpectedToken { expected, found } => {
                write!(f, "Expected token {} - found {}", expected, found)
            }
            ParseErrors::ExpectedWord { expected, found } => {
                write!(f, "Expected word {} - found {}", expected, found)
            }
            ParseErrors::EnumeratorNotFound(_name) => {
                write!(f, "Enumerator not found: working on it :)")
            }
            ParseErrors::ExpectedToNotBe(kind) => write!(f, "Expected to not be {:?}", kind),
            ParseErrors::VariableNotFound(_name) => {
                write!(f, "Variable not found: working on it :)")
            }
            ParseErrors::UncountableVariable(_name, kind) => {
                write!(f, "Uncountable variable: dont know<{:?}>", kind)
            }
            ParseErrors::CannotSetVariable(_name, kind) => {
                write!(f, "Cannot set variable: dont know<{:?}>", kind)
            }
            ParseErrors::Message(err) => write!(f, "{}", err.msg),
            ParseErrors::Eof => write!(f, "Unexpected end of file"),
            ParseErrors::LabelNotFound(name) => write!(f, "Label not found: {}", name),
            ParseErrors::CannotGoBack(steps) => write!(f, "Cannot go back {} steps", steps),
            ParseErrors::CannotBreak(n) => write!(f, "Cannot break {} more steps", n),
            ParseErrors::ExpectedOneOf { expected, found } => {
                write!(f, "Expected one of {:?} - found {}", expected, found)
            }
            ParseErrors::CouldNotFindToken(kind) => write!(f, "Could not find token {:?}", kind),
            ParseErrors::Ok => write!(f, "If you see this, it could be a bug in the parser"),
            ParseErrors::MissingEof(found) => write!(
                f,
                "Could not parse to the end of the file - found {}",
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

#[derive(Debug, Clone)]
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
