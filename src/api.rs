use crate::{
    lexer::{TextLocation, Token},
    parser::{self, Nodes},
};

// Choose between std and alloc
cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        extern crate std;
        use std::prelude::v1::*;
    } else {
        extern crate alloc;
        use alloc::string::*;
        use alloc::vec::*;
        use alloc::vec;
        use core::fmt;
        use alloc::format;
    }
}

impl<'a> parser::Nodes<'a> {
    /// Returns name of node
    ///
    /// Panics if the type is token
    pub fn name(&'a self) -> &'a str {
        match self {
            parser::Nodes::Node(node) => node.name,
            parser::Nodes::Token(tok) => panic!("No name found for token: {:?}", tok.kind),
        }
    }

    /// Returns token type
    ///
    /// Panics if the type is node
    pub fn token(&'a self) -> &'a Token {
        match self {
            parser::Nodes::Node(node) => panic!("No token found for node: {:?}", node.name),
            parser::Nodes::Token(tok) => tok,
        }
    }

    /// The length in text
    pub fn len(&self) -> usize {
        match self {
            parser::Nodes::Node(node) => node.last_string_idx - node.first_string_idx,
            parser::Nodes::Token(tok) => tok.len,
        }
    }

    /// Returns value of variable that is a number
    ///
    /// Panics if the variable is not a number or if it does not exist
    pub fn get_number(&self, variable: &str) -> i32 {
        match self {
            parser::Nodes::Node(node) => node.get_number(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    /// Returns value of variable that is a bool
    ///
    /// Panics if the variable is not a bool or if it does not exist
    pub fn get_bool(&self, variable: &str) -> bool {
        match self {
            parser::Nodes::Node(node) => node.get_bool(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    /// Returns value of variable that is a node
    ///
    /// Panics if the variable is not a node or if it does not exist
    pub fn try_get_node(&self, variable: &str) -> &Option<parser::Nodes<'_>> {
        match self {
            parser::Nodes::Node(node_) => node_.try_get_node(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    /// Returns value of variable that is a list of nodes
    ///
    /// Panics if the variable is not a list of nodes or if it does not exist
    pub fn get_list(&self, variable: &str) -> &Vec<parser::Nodes<'_>> {
        match self {
            parser::Nodes::Node(node_) => node_.get_list(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    pub fn location(&self) -> TextLocation {
        match self {
            parser::Nodes::Node(node) => node.location,
            parser::Nodes::Token(tok) => tok.location,
        }
    }
}

impl<'a> parser::Node<'a> {
    /// Returns value of variable that is a number
    ///
    /// Panics if the variable is not a number or if it does not exist
    pub fn get_number(&self, variable: &str) -> i32 {
        match self.variables.get(variable) {
            Some(num) => match num {
                &parser::VariableKind::Number(num) => num,
                _ => panic!("Variable <key> is not a number for node",),
            },
            None => panic!("No variable <key> found for node: {:?}", self.name),
        }
    }

    /// Returns value of variable that is a bool
    ///
    /// Panics if the variable is not a bool or if it does not exist
    pub fn get_bool(&self, variable: &str) -> bool {
        match self.variables.get(variable) {
            Some(bool) => match bool {
                &parser::VariableKind::Boolean(bool) => bool,
                _ => panic!("Variable <aaaa> is not a bool for node: {:?}", self.name),
            },
            None => panic!("No variable <aaa> found for node: {:?}", self.name),
        }
    }

    /// Returns value of variable that is a node
    ///
    /// Panics if the variable is not a node or if it does not exist
    pub fn try_get_node(&self, variable: &str) -> &Option<parser::Nodes<'_>> {
        match self.variables.get(variable) {
            Some(node) => match node {
                parser::VariableKind::Node(ref node) => node,
                _ => panic!("Variable <fsdg> is not a node for node: {:?}", self.name),
            },
            None => panic!("No variable <asdfasdf> found for node: {:?}", self.name),
        }
    }

    /// Returns value of variable that is a list of nodes
    ///
    /// Panics if the variable is not a list of nodes or if it does not exist
    pub fn get_list(&self, variable: &str) -> &Vec<parser::Nodes<'_>> {
        match self.variables.get(variable) {
            Some(ref array) => match array {
                parser::VariableKind::NodeList(array) => array,
                _ => panic!(
                    "Variable <asdgfasdf> is not an array for node: {:?}",
                    self.name
                ),
            },
            None => panic!("No variable <sadtgeas> found for node: {:?}", self.name),
        }
    }
}

impl<'a> parser::ParseResult<'a> {
    /// Returns stringified version of the node
    ///
    /// This operation is O(1)
    pub fn stringify_node(node: &parser::Nodes, text: &'a str) -> &'a str {
        match node {
            parser::Nodes::Node(node) => &text[node.first_string_idx..node.last_string_idx],
            parser::Nodes::Token(tok) => &text[tok.index..tok.index + tok.len],
        }
    }

    /// Returns stringified version of the node
    ///
    /// This operation is O(1)
    pub fn stringify_nodes_range(
        &self,
        start: &parser::Nodes,
        end: &parser::Nodes,
        text: &'a str,
    ) -> &'a str {
        let start_idx = match start {
            parser::Nodes::Node(node) => node.first_string_idx,
            parser::Nodes::Token(tok) => tok.index,
        };
        let end_idx = match end {
            parser::Nodes::Node(node) => node.last_string_idx,
            parser::Nodes::Token(tok) => tok.index + tok.len,
        };
        &text[start_idx..end_idx]
    }
}

impl<'a> Nodes<'a> {
    pub fn stringify(&self, txt: &'a str) -> &'a str {
        match self {
            Nodes::Node(node) => &txt[node.first_string_idx..node.last_string_idx],
            Nodes::Token(token) => &txt[token.index..token.index + token.len],
        }
    }

    pub fn stringify_until(&self, end: &Self, txt: &'a str) -> &'a str {
        let end = match end {
            Nodes::Node(node) => node.last_string_idx,
            Nodes::Token(token) => token.index + token.len,
        };
        match self {
            Nodes::Node(node) => &txt[node.first_string_idx..end],
            Nodes::Token(token) => &txt[token.index..end],
        }
    }
}

pub mod ext {

    use smol_str::SmolStr;

    use crate::{
        grammar::{
            Commands, Comparison, Enumerator, ErrorDefinition, Grammar, MatchToken, Node, OneOf,
            Parameters, Rule, VarKind, VariableKind,
        },
        lexer::{ControlTokenKind, TokenKinds},
    };

    pub fn token(tok: impl Into<SmolStr>) -> MatchToken<'static> {
        MatchToken::Token(TokenKinds::Token(tok.into()))
    }

    pub fn word<'a>(word: &'a str) -> MatchToken<'a> {
        MatchToken::Word(word)
    }

    pub fn text() -> MatchToken<'static> {
        MatchToken::Token(TokenKinds::Text)
    }

    pub fn whitespace() -> MatchToken<'static> {
        MatchToken::Token(TokenKinds::Whitespace)
    }

    pub fn any() -> MatchToken<'static> {
        MatchToken::Any
    }

    pub fn node<'a>(node: &'a str) -> MatchToken<'a> {
        MatchToken::Node(node)
    }

    pub fn enumerator<'a>(enumerator: &'a str) -> MatchToken<'a> {
        MatchToken::Enumerator(enumerator)
    }

    pub fn newline() -> MatchToken<'static> {
        MatchToken::Token(TokenKinds::Control(ControlTokenKind::Eol))
    }

    pub fn eof() -> MatchToken<'static> {
        MatchToken::Token(TokenKinds::Control(ControlTokenKind::Eof))
    }

    pub fn is<'a>(matches: MatchToken<'a>) -> Rule<'a> {
        Rule::Is {
            token: matches,
            rules: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub fn isnt<'a>(matches: MatchToken<'a>) -> Rule<'a> {
        Rule::Isnt {
            token: matches,
            rules: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub fn maybe<'a>(matches: MatchToken<'a>) -> Rule<'a> {
        Rule::Maybe {
            token: matches,
            parameters: Vec::new(),
            is: Vec::new(),
            isnt: Vec::new(),
        }
    }

    pub fn while_<'a>(matches: MatchToken<'a>) -> Rule<'a> {
        Rule::While {
            token: matches,
            rules: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub fn loop_<'a>() -> Rule<'a> {
        Rule::Loop { rules: Vec::new() }
    }

    pub fn maybe_one_of<'a>(options: impl IntoIterator<Item = OneOf<'a>>) -> Rule<'a> {
        Rule::MaybeOneOf {
            is_one_of: options.into_iter().collect(),
            isnt: Vec::new(),
        }
    }

    pub fn is_one_of<'a>(options: impl IntoIterator<Item = OneOf<'a>>) -> Rule<'a> {
        Rule::IsOneOf {
            tokens: options.into_iter().collect(),
            parameters: Vec::new(),
        }
    }

    pub fn until<'a>(matches: MatchToken<'a>) -> Rule<'a> {
        Rule::Until {
            token: matches,
            rules: Vec::new(),
            parameters: Vec::new(),
        }
    }

    pub fn compare<'a>(a: VarKind<'a>, b: VarKind<'a>, comp: Comparison) -> Rule<'a> {
        Rule::Command {
            command: Commands::Compare {
                left: a,
                right: b,
                comparison: comp,
                rules: Vec::new(),
            },
        }
    }

    pub fn print_msg<'a>(msg: &'a str) -> Rule<'a> {
        Rule::Command {
            command: Commands::Print { message: msg },
        }
    }

    pub fn goto<'a>(label: &'a str) -> Rule<'a> {
        Rule::Command {
            command: Commands::Goto { label },
        }
    }

    pub fn commit() -> Rule<'static> {
        Rule::Command {
            command: Commands::Commit { set: true },
        }
    }

    pub fn label<'a>(identifier: &'a str) -> Rule<'a> {
        Rule::Command {
            command: Commands::Label { name: identifier },
        }
    }

    pub fn return_node<'a>() -> Rule<'a> {
        Rule::Command {
            command: Commands::Return,
        }
    }

    pub fn start<'a>() -> Rule<'a> {
        Rule::Command {
            command: Commands::Start,
        }
    }

    pub fn end<'a>() -> Rule<'a> {
        Rule::Command {
            command: Commands::End,
        }
    }

    impl<'a> Rule<'a> {
        pub fn params(mut self, params: impl IntoIterator<Item = Parameters<'a>>) -> Self {
            match &mut self {
                Rule::Is { parameters, .. } | Rule::Isnt { parameters, .. } => {
                    parameters.extend(params);
                }
                Rule::Maybe { parameters, .. } => parameters.extend(params),
                Rule::While { parameters, .. }
                | Rule::Until { parameters, .. }
                | Rule::IsOneOf { parameters, .. } => {
                    parameters.extend(params);
                }
                _ => panic!("Can not set params for rule: {:?}", self),
            }
            self
        }

        pub fn then(mut self, set_rules: impl IntoIterator<Item = Rule<'a>>) -> Self {
            match &mut self {
                Self::Is { rules, .. } | Self::Isnt { rules, .. } => rules.extend(set_rules),
                Self::While { rules, .. } | Self::Until { rules, .. } => rules.extend(set_rules),
                Self::Maybe { is, .. } => is.extend(set_rules),
                Self::Loop { rules } => rules.extend(set_rules),
                Self::Command {
                    command: Commands::Compare { rules, .. },
                } => rules.extend(set_rules),
                _ => panic!("Can not set 'then' rules for rule: {:?}", self),
            }
            self
        }

        pub fn otherwise(mut self, set_rules: impl IntoIterator<Item = Rule<'a>>) -> Self {
            match &mut self {
                Self::Maybe { isnt, .. } => isnt.extend(set_rules),
                _ => panic!("Can not set 'otherwise' rulse for rule: {:?}", self),
            }
            self
        }

        pub fn set(self, var: VarKind<'a>) -> Self {
            self.params([Parameters::Set(var)])
        }

        pub fn fail(self, err: &'a ErrorDefinition) -> Self {
            self.params([Parameters::Fail(err)])
        }

        pub fn goto(self, msg: &'a str) -> Self {
            self.params([Parameters::Goto(msg)])
        }

        pub fn inc(self, var: VarKind<'a>) -> Self {
            self.params([Parameters::Increment(var)])
        }

        pub fn dec(self, var: VarKind<'a>) -> Self {
            self.params([Parameters::Decrement(var)])
        }

        pub fn clone_value(self, src: VarKind<'a>, dst: VarKind<'a>) -> Self {
            self.params([Parameters::CloneValue(src, dst)])
        }

        pub fn debug_var(self, var: VarKind<'a>) -> Self {
            self.params([Parameters::Debug(Some(var))])
        }

        pub fn debug_token(self) -> Self {
            self.params([Parameters::Debug(None)])
        }

        pub fn commit(self) -> Self {
            self.params([Parameters::Commit(true)])
        }

        pub fn print(self, txt: &'a str) -> Self {
            self.params([Parameters::Print(txt)])
        }

        pub fn hint(self, txt: &'a str) -> Self {
            self.params([Parameters::Hint(txt)])
        }

        pub fn start(self) -> Self {
            self.params([Parameters::NodeStart])
        }

        pub fn end(self) -> Self {
            self.params([Parameters::NodeEnd])
        }

        pub fn return_node(self) -> Self {
            self.params([Parameters::Return])
        }
    }

    pub const fn local<'a>(name: &'a str) -> VarKind<'a> {
        VarKind::Local(name)
    }

    pub fn global<'a>(name: &'a str) -> VarKind<'a> {
        VarKind::Global(name)
    }

    pub fn options<'a>(options: impl IntoIterator<Item = OneOf<'a>>) -> Vec<OneOf<'a>> {
        options.into_iter().collect()
    }

    pub fn rules<'a>(rules: impl IntoIterator<Item = Rule<'a>>) -> Vec<Rule<'a>> {
        rules.into_iter().collect()
    }

    pub fn variables<'a>(
        variables: impl IntoIterator<Item = (&'a str, VariableKind)>,
    ) -> Vec<(&'a str, VariableKind)> {
        variables.into_iter().collect()
    }

    pub fn node_var(name: &str) -> (&str, VariableKind) {
        (name, VariableKind::Node)
    }

    pub fn list_var(name: &str) -> (&str, VariableKind) {
        (name, VariableKind::NodeList)
    }

    pub fn number_var(name: &str) -> (&str, VariableKind) {
        (name, VariableKind::Number)
    }

    pub fn bool_var(name: &str) -> (&str, VariableKind) {
        (name, VariableKind::Boolean)
    }

    pub fn option<'a>(matches: MatchToken<'a>) -> OneOf<'a> {
        OneOf {
            token: matches,
            rules: Vec::new(),
            parameters: Vec::new(),
        }
    }

    impl<'a> OneOf<'a> {
        pub fn then(mut self, set_rules: impl IntoIterator<Item = Rule<'a>>) -> Self {
            self.rules = set_rules.into_iter().collect();
            self
        }

        pub fn params(mut self, params: impl IntoIterator<Item = Parameters<'a>>) -> Self {
            self.parameters = params.into_iter().collect();
            self
        }

        pub fn set(self, var: VarKind<'a>) -> Self {
            self.params([Parameters::Set(var)])
        }

        pub fn fail(self, err: &'a ErrorDefinition) -> Self {
            self.params([Parameters::Fail(err)])
        }

        pub fn goto(self, msg: &'a str) -> Self {
            self.params([Parameters::Goto(msg)])
        }

        pub fn debug_var(self, var: VarKind<'a>) -> Self {
            self.params([Parameters::Debug(Some(var))])
        }

        pub fn debug_token(self) -> Self {
            self.params([Parameters::Debug(None)])
        }

        pub fn clone_value(self, src: VarKind<'a>, dst: VarKind<'a>) -> Self {
            self.params([Parameters::CloneValue(src, dst)])
        }

        pub fn commit(self) -> Self {
            self.params([Parameters::Commit(true)])
        }

        pub fn print(self, txt: &'a str) -> Self {
            self.params([Parameters::Print(txt)])
        }

        pub fn hint(self, txt: &'a str) -> Self {
            self.params([Parameters::Hint(txt)])
        }

        pub fn start(self) -> Self {
            self.params([Parameters::NodeStart])
        }

        pub fn end(self) -> Self {
            self.params([Parameters::NodeEnd])
        }

        pub fn return_node(self) -> Self {
            self.params([Parameters::Return])
        }
    }

    pub struct NodeBuilder<'g, 'a> {
        grammar: &'g mut Grammar<'a>,
        pub name: &'a str,
        pub rules: Vec<Rule<'a>>,
        pub variables: Vec<(&'a str, VariableKind)>,
        pub docs: Option<&'a str>,
    }

    pub struct EnumBuilder<'g, 'a> {
        grammar: &'g mut Grammar<'a>,
        pub name: &'a str,
        pub options: Vec<MatchToken<'a>>,
    }

    impl<'a> Grammar<'a> {
        pub fn new_node<'g>(&'g mut self, name: &'a str) -> NodeBuilder<'g, 'a> {
            NodeBuilder {
                grammar: self,
                name,
                rules: Vec::new(),
                variables: Vec::new(),
                docs: None,
            }
        }

        pub fn new_enum<'g>(&'g mut self, name: &'a str) -> EnumBuilder<'g, 'a> {
            EnumBuilder {
                grammar: self,
                name,
                options: Vec::new(),
            }
        }
    }

    impl<'g, 'a> NodeBuilder<'g, 'a> {
        pub fn name(mut self, name: &'a str) -> Self {
            self.name = name;
            self
        }

        pub fn rules(mut self, rules: impl IntoIterator<Item = Rule<'a>>) -> Self {
            self.rules.extend(rules);
            self
        }

        pub fn variables(
            mut self,
            variables: impl IntoIterator<Item = (&'a str, VariableKind)>,
        ) -> Self {
            self.variables.extend(variables);
            self
        }

        pub fn docs(mut self, text: &'a str) -> Self {
            self.docs = Some(text);
            self
        }

        pub fn build(self) -> MatchToken<'a> {
            let n = Node {
                name: self.name,
                rules: self.rules,
                variables: self.variables,
                docs: self.docs,
            };
            self.grammar.add_node(n);
            node(self.name)
        }
    }

    impl<'g, 'a> EnumBuilder<'g, 'a> {
        pub fn name(mut self, name: &'a str) -> Self {
            self.name = name;
            self
        }

        pub fn options(mut self, options: impl IntoIterator<Item = MatchToken<'a>>) -> Self {
            self.options.extend(options);
            self
        }

        pub fn build(self) -> MatchToken<'a> {
            let e = Enumerator {
                name: self.name,
                values: self.options,
            };
            self.grammar.add_enum(e);
            enumerator(self.name)
        }
    }
}
