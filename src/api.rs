use arena::Key;

use crate::{
    grammar::VariableTag,
    lexer::{TextLocation, Token},
    parser,
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
            parser::Nodes::Node(node) => &node.name,
            parser::Nodes::Token(tok) => panic!("No name found for token: {:?}", tok.kind),
        }
    }

    /// Returns token type
    ///
    /// Panics if the type is node
    pub fn token(&'a self) -> &'a Token {
        match self {
            parser::Nodes::Node(node) => panic!("No token found for node: {:?}", node.name),
            parser::Nodes::Token(tok) => &tok,
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
    pub fn get_number(&self, variable: &Key<VariableTag>) -> i32 {
        match self {
            parser::Nodes::Node(node) => node.get_number(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    /// Returns value of variable that is a bool
    ///
    /// Panics if the variable is not a bool or if it does not exist
    pub fn get_bool(&self, variable: &Key<VariableTag>) -> bool {
        match self {
            parser::Nodes::Node(node) => node.get_bool(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    /// Returns value of variable that is a node
    ///
    /// Panics if the variable is not a node or if it does not exist
    pub fn try_get_node(&self, variable: &Key<VariableTag>) -> &Option<parser::Nodes> {
        match self {
            parser::Nodes::Node(node_) => node_.try_get_node(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    /// Returns value of variable that is a list of nodes
    ///
    /// Panics if the variable is not a list of nodes or if it does not exist
    pub fn get_list(&self, variable: &Key<VariableTag>) -> &Vec<parser::Nodes> {
        match self {
            parser::Nodes::Node(node_) => node_.get_list(variable),
            parser::Nodes::Token(tok) => panic!("No variables found for token: {:?}", tok.kind),
        }
    }

    pub fn location(&self) -> TextLocation {
        match self {
            parser::Nodes::Node(node) => node.location.clone(),
            parser::Nodes::Token(tok) => tok.location.clone(),
        }
    }
}

impl<'a> parser::Node<'a> {
    /// Returns value of variable that is a number
    ///
    /// Panics if the variable is not a number or if it does not exist
    pub fn get_number(&self, variable: &Key<VariableTag>) -> i32 {
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
    pub fn get_bool(&self, variable: &Key<VariableTag>) -> bool {
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
    pub fn try_get_node(&self, variable: &Key<VariableTag>) -> &Option<parser::Nodes> {
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
    pub fn get_list(&self, variable: &Key<VariableTag>) -> &Vec<parser::Nodes> {
        match self.variables.get(variable) {
            Some(ref array) => match array {
                parser::VariableKind::NodeList(array) => &array,
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
