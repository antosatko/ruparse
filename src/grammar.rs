use crate::Map;

use crate::lexer::TokenKinds;

use arena::{Arena, Key};
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;

// Choose between std and alloc
cfg_if::cfg_if! {
    if #[cfg(feature = "std")] {
        extern crate std;
        use std::prelude::v1::*;
    } else {
        extern crate alloc;
        use alloc::string::*;
        use alloc::vec::*;
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct NodeTag;
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct EnumeratorTag;
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct VariableTag;
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct GlobalVariableTag;

#[derive(Debug, Clone)]
pub struct Grammar<'a> {
    pub nodes: Arena<Node<'a>, NodeTag>,
    pub enumerators: Arena<Enumerator<'a>, EnumeratorTag>,
    pub globals: Arena<VariableKind, GlobalVariableTag>,
    /// If true, the parser will throw an error if the last token is not EOF
    pub eof: bool,
}

impl<'a> Grammar<'a> {
    pub fn new() -> Grammar<'a> {
        Grammar {
            nodes: Arena::new(),
            enumerators: Arena::new(),
            globals: Arena::new(),
            eof: true,
        }
    }

    pub fn add_node(&mut self, node: Node<'a>) -> Key<NodeTag> {
        self.nodes.push(node)
    }
}

/// A collection of rules
pub type Rules<'a> = Vec<Rule<'a>>;

/// A rule defines how a token will be matched and what will happen if it is matched
///
/// It also contains parameters that can be used if the rule is matched
///
/// Special kind of rules are commands that can be executed without matching a token
#[derive(Debug, Clone)]
pub enum Rule<'a> {
    /// Matches a token
    ///
    /// If the token is matched, the rules will be executed
    ///
    /// If the token is not matched, the node will end with an error
    Is {
        token: MatchToken<'a>,
        rules: Rules<'a>,
        parameters: Vec<Parameters<'a>>,
    },
    /// Matches a token
    ///
    /// If the token is matched, the node will end with an error
    ///
    /// If the token is not matched, the rules will be executed
    Isnt {
        token: MatchToken<'a>,
        rules: Rules<'a>,
        parameters: Vec<Parameters<'a>>,
    },
    /// Matches one of the tokens
    ///
    /// If one of the tokens is matched, the rules will be executed
    ///
    /// If none of the tokens is matched, the node will end with an error
    IsOneOf {
        tokens: Vec<OneOf<'a>>,
    },
    /// Matches a token
    ///
    /// If the token is matched, the rules will be executed
    ///
    /// If the token is not matched, the rules for the else branch will be executed
    Maybe {
        /// Token that will be matched
        token: MatchToken<'a>,
        /// Rules that will be executed if the token is matched
        is: Rules<'a>,
        /// Rules that will be executed if the token is not matched
        isnt: Rules<'a>,
        /// Parameters that can be used if the token is matched
        parameters: Vec<Parameters<'a>>,
    },
    /// Matches one of the tokens
    ///
    /// If one of the tokens is matched, the rules will be executed
    ///
    /// If none of the tokens is matched, the rules for the else branch will be executed
    MaybeOneOf {
        /// Tokens that will be matched
        is_one_of: Vec<OneOf<'a>>,
        /// Rules that will be executed if none of the tokens is matched
        isnt: Rules<'a>,
    },
    /// Matches a token
    ///
    /// If the token is matched, the rules will be executed
    ///
    /// After the rules are executed, the token will be matched again
    /// and the rules will be executed again (if the token is matched)
    While {
        token: MatchToken<'a>,
        rules: Rules<'a>,
        /// Parameters that can be used if the token is matched
        ///
        /// The parameters will be used once every time the token is matched
        parameters: Vec<Parameters<'a>>,
    },
    /// Loop that will be executed until a break command is executed
    Loop {
        rules: Rules<'a>,
    },
    /// Searches in the tokens until a token is matched
    Until {
        token: MatchToken<'a>,
        rules: Rules<'a>,
        parameters: Vec<Parameters<'a>>,
    },
    /// Searches in the tokens until one of the tokens is matched
    UntilOneOf {
        tokens: Vec<OneOf<'a>>,
    },
    /// Performs a command
    ///
    /// The command will be executed without matching a token
    Command {
        command: Commands<'a>,
    },
    Debug {
        target: Option<&'a str>,
    },
}

/// One of the tokens that will be matched
#[derive(Debug, Clone)]
pub struct OneOf<'a> {
    pub token: MatchToken<'a>,
    pub rules: Rules<'a>,
    pub parameters: Vec<Parameters<'a>>,
}

#[derive(Debug, Clone, Copy)]
pub enum VarKind {
    Local(Key<VariableTag>),
    Global(Key<GlobalVariableTag>),
}

/// Commands that can be executed
#[derive(Debug, Clone)]
pub enum Commands<'a> {
    /// Compares two variables/numbers and executes rules if the comparison is true
    Compare {
        /// Left side of the comparison
        left: VarKind,
        /// Right side of the comparison
        right: VarKind,
        /// Comparison operator
        comparison: Comparison,
        /// Rules that will be executed if the comparison is true
        rules: Rules<'a>,
    },
    /// Returns an error from node
    Error {
        message: &'a str,
    },
    HardError {
        set: bool,
    },
    Goto {
        label: &'a str,
    },
    Label {
        name: &'a str,
    },
    Print {
        message: &'a str,
    },
}

/// Comparison operators
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum Comparison {
    /// ==
    Equal,
    /// !=
    NotEqual,
    /// >
    GreaterThan,
    /// <
    LessThan,
    /// >=
    GreaterThanOrEqual,
    /// <=
    LessThanOrEqual,
}

/// A token that will be matched
///
/// Can be a token kind or a node name
#[derive(Clone, Debug)]
pub enum MatchToken<'a> {
    /// A token kind
    Token(TokenKinds),
    /// A node name
    Node(Key<NodeTag>),
    /// A constant word
    Word(&'a str),
    /// An enumerator
    Enumerator(Key<EnumeratorTag>),
    /// Any token
    Any,
}

/// A node is a collection of rules that will be executed when the node is matched
#[derive(Debug, Clone)]
pub struct Node<'a> {
    /// Name of the node
    pub name: &'a str,
    /// Rules that will be executed when the node is matched
    pub rules: Rules<'a>,
    /// Variables that can be used in the node and will be accessible from the outside
    pub variables: Arena<VariableKind, VariableTag>,
    /// Documentation for the node
    pub docs: Option<&'a str>,
}

/// A variable that can be used in a node
#[derive(Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum VariableKind {
    /// Holds a single node
    Node,
    /// Holds a list of nodes
    NodeList,
    /// Holds a boolean
    Boolean,
    /// Holds a number
    Number,
}

/// Parameters that can be used on a rule if it is matched
#[derive(Debug, Clone)]
pub enum Parameters<'a> {
    /// Sets a variable to a value
    Set(Key<VariableTag>),
    /// Sets a global variable to a value
    Global(Key<GlobalVariableTag>),
    /// Adds 1 to a variable of type Count
    Increment(Key<VariableTag>),
    /// Subtracts 1 from a variable of type Count
    Decrement(Key<VariableTag>),
    /// Adds 1 to a global variable of type Count
    IncrementGlobal(Key<GlobalVariableTag>),
    /// Sets a variable to true
    True(Key<VariableTag>),
    /// Sets a variable to false
    False(Key<VariableTag>),
    /// Sets a global variable to true
    TrueGlobal(Key<GlobalVariableTag>),
    /// Sets a global variable to false
    FalseGlobal(Key<GlobalVariableTag>),
    /// Prints string
    Print(&'a str),
    /// Prints current token or variable
    Debug(Option<Key<VariableTag>>),
    /// Goes back in rules
    Back(u8),
    /// Returns from node
    Return,
    /// Breaks from rule blocks(n)
    Break(usize),
    /// If the node ends with an error, it will be a hard error
    /// resulting in the parent node to also end with an error
    ///
    /// This is a way of telling the parser that the current node MUST match
    ///
    /// This is useful for using nodes in optional rules
    HardError(bool),
    /// Sets the current node to the label with the given name
    Goto(&'a str),
    /// Hints to the parser that the node starts here
    ///
    /// This should be used at the start of every node
    /// because it will prevent the parser from counting
    /// whitespace in front of the node
    NodeStart,
    /// Hints to the parser that the node ends here
    NodeEnd,
}

#[derive(Debug, Clone)]
pub struct Enumerator<'a> {
    pub name: &'a str,
    pub values: Vec<MatchToken<'a>>,
}

/// validation module for grammar that is otherwise dynamically typed
///
/// This module is used to validate the grammar and make sure that it is correct
///
/// The grammar is validated by checking if the rules are correct and if the variables are used correctly
///
/// > note: Grammar errors have caused me a lot of headache in the past so using this module is highly recommended
pub mod validator {

    use smol_str::SmolStr;

    use super::*;
    use crate::lexer::*;

    impl Lexer {
        pub fn validate_tokens(&self, result: &mut ValidationResult) {
            let mut tokens: Vec<SmolStr> = Vec::new();
            for token in &self.token_kinds {
                // tokens that have already been validated can be ignored
                if tokens.contains(token) {
                    continue;
                }
                tokens.push(token.clone());
                // check for collisions
                if self.token_kinds.iter().filter(|t| *t == token).count() > 1 {
                    result.errors.push(ValidationError {
                        kind: ValidationErrors::TokenCollision(SmolStr::new(token)),
                        node_name: "__lexer__",
                    });
                }
                // check if token is empty
                if token.is_empty() {
                    result.errors.push(ValidationError {
                        kind: ValidationErrors::EmptyToken,
                        node_name: "__lexer__",
                    });
                }
                // check if it starts with a number
                let first = token.chars().next().unwrap();
                if first.is_numeric() {
                    result.warnings.push(ValidationWarning {
                        kind: ValidationWarnings::UnusualToken(
                            SmolStr::new(token),
                            TokenErrors::StartsNumeric,
                        ),
                        node_name: "__lexer__",
                    });
                }

                // check if it contains a whitespace
                if token.chars().any(|c| c.is_whitespace()) {
                    result.warnings.push(ValidationWarning {
                        kind: ValidationWarnings::UnusualToken(
                            SmolStr::new(token),
                            TokenErrors::ContainsWhitespace,
                        ),
                        node_name: "__lexer__",
                    });
                }

                // check if it is longer than 2 characters
                if token.len() > 2 {
                    result.warnings.push(ValidationWarning {
                        kind: ValidationWarnings::UnusualToken(
                            SmolStr::new(token),
                            TokenErrors::TooLong,
                        ),
                        node_name: "__lexer__",
                    });
                }

                // check if it is not ascii
                if !token.chars().all(|c| c.is_ascii()) {
                    result.warnings.push(ValidationWarning {
                        kind: ValidationWarnings::UnusualToken(
                            SmolStr::new(token),
                            TokenErrors::NotAscii,
                        ),
                        node_name: "__lexer__",
                    });
                }
            }
        }
    }

    // impl<'a> Grammar<'a> {
    //     /// Validates the grammar
    //     pub fn validate(&self, lexer: &Lexer) -> ValidationResult {
    //         let mut result = ValidationResult::new();
    //         lexer.validate_tokens(&mut result);

    //         for node in self.nodes.iter() {
    //             self.validate_node(node, lexer, &mut result);
    //         }

    //         result
    //     }

    //     pub fn validate_node(
    //         &self,
    //         node: &'a Node,
    //         lexer: &Lexer,
    //         result: &mut ValidationResult<'a>,
    //     ) {
    //         let mut laf = LostAndFound::new();
    //         for rule in &node.rules {
    //             self.validate_rule(rule, node, lexer, &mut laf, result);
    //         }
    //         laf.pass(result, &node.name);
    //     }

    //     pub fn validate_rule(
    //         &self,
    //         rule: &'a Rule,
    //         node: &Node<'a>,
    //         lexer: &Lexer,
    //         laf: &mut LostAndFound<'a>,
    //         result: &mut ValidationResult<'a>,
    //     ) {
    //         match rule {
    //             Rule::Is {
    //                 token,
    //                 rules,
    //                 parameters,
    //             } => {
    //                 self.validate_token(token, node, lexer, laf, result);
    //                 self.validate_parameters(parameters, node, laf, result);
    //                 self.validate_ruleblock(rules, node, lexer, laf, result)
    //             }
    //             Rule::Isnt {
    //                 token,
    //                 rules,
    //                 parameters,
    //             } => {
    //                 self.validate_token(token, node, lexer, laf, result);
    //                 self.validate_parameters(parameters, node, laf, result);
    //                 self.validate_ruleblock(rules, node, lexer, laf, result)
    //             }
    //             Rule::IsOneOf { tokens } => {
    //                 for one_of in tokens {
    //                     self.validate_token(&one_of.token, node, lexer, laf, result);
    //                     self.validate_parameters(&one_of.parameters, node, laf, result);
    //                     self.validate_ruleblock(&one_of.rules, node, lexer, laf, result)
    //                 }
    //             }
    //             Rule::Maybe {
    //                 token,
    //                 is,
    //                 isnt,
    //                 parameters,
    //             } => {
    //                 self.validate_token(token, node, lexer, laf, result);
    //                 self.validate_parameters(parameters, node, laf, result);
    //                 self.validate_ruleblock(is, node, lexer, laf, result);
    //                 self.validate_ruleblock(isnt, node, lexer, laf, result);
    //             }
    //             Rule::MaybeOneOf { is_one_of, isnt } => {
    //                 for OneOf {
    //                     token,
    //                     rules,
    //                     parameters,
    //                 } in is_one_of
    //                 {
    //                     self.validate_token(token, node, lexer, laf, result);
    //                     self.validate_parameters(parameters, node, laf, result);
    //                     self.validate_ruleblock(rules, node, lexer, laf, result);
    //                 }
    //                 self.validate_ruleblock(isnt, node, lexer, laf, result);
    //             }
    //             Rule::While {
    //                 token,
    //                 rules,
    //                 parameters,
    //             } => {
    //                 self.validate_token(token, node, lexer, laf, result);
    //                 self.validate_parameters(parameters, node, laf, result);
    //                 self.validate_ruleblock(rules, node, lexer, laf, result)
    //             }
    //             Rule::Loop { rules } => self.validate_ruleblock(rules, node, lexer, laf, result),
    //             Rule::Until {
    //                 token,
    //                 rules,
    //                 parameters,
    //             } => {
    //                 self.validate_token(token, node, lexer, laf, result);
    //                 self.validate_parameters(parameters, node, laf, result);
    //                 self.validate_ruleblock(rules, node, lexer, laf, result)
    //             }
    //             Rule::UntilOneOf { tokens } => {
    //                 for one_of in tokens {
    //                     self.validate_token(&one_of.token, node, lexer, laf, result);
    //                     self.validate_parameters(&one_of.parameters, node, laf, result);
    //                     self.validate_ruleblock(&one_of.rules, node, lexer, laf, result)
    //                 }
    //             }
    //             Rule::Command { command } => match command {
    //                 Commands::Compare {
    //                     left,
    //                     right,
    //                     comparison: _,
    //                     rules,
    //                 } => {
    //                     use VarKind::*;
    //                     let mut cant_use_err;
    //                     let l = match left {
    //                         Local(ll) => {
    //                             cant_use_err = ValidationErrors::CantUseVariable(*ll);
    //                             node.variables.get_unchecked(ll)
    //                         }
    //                         Global(gl) => {
    //                             cant_use_err = ValidationErrors::CantUseGlobalVariable(*gl);
    //                             self.globals.get_unchecked(gl)
    //                         }
    //                     };
    //                     match l {
    //                         VariableKind::Number => (),
    //                         _ => result.errors.push(ValidationError {
    //                             kind: cant_use_err,
    //                             node_name: node.name.into(),
    //                         }),
    //                     };
    //                     match self.globals.get(*right) {
    //                         Some(var) => match var {
    //                             VariableKind::Number => (),
    //                             _ => result.errors.push(ValidationError {
    //                                 kind: ValidationErrors::CantUseVariable(right.clone()),
    //                                 node_name: node.name.clone(),
    //                             }),
    //                         },
    //                         None => match node.variables.get(*right) {
    //                             Some(var) => match var {
    //                                 VariableKind::Number => (),
    //                                 _ => result.errors.push(ValidationError {
    //                                     kind: ValidationErrors::CantUseVariable(right.clone()),
    //                                     node_name: node.name.clone(),
    //                                 }),
    //                             },
    //                             None => {
    //                                 result.errors.push(ValidationError {
    //                                     kind: ValidationErrors::GlobalNotFound(right.clone()),
    //                                     node_name: node.name.clone(),
    //                                 });
    //                             }
    //                         },
    //                     }
    //                     for rule in rules {
    //                         self.validate_rule(rule, node, lexer, laf, result);
    //                     }
    //                 }
    //                 Commands::Error { message: _ } => (),
    //                 Commands::HardError { set: _ } => (),
    //                 Commands::Goto { label } => {
    //                     laf.lost_labels.push(label.clone());
    //                 }
    //                 Commands::Label { name } => {
    //                     if laf.found_labels.contains(&name) {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::DuplicateLabel(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                     laf.found_labels.push(&name);
    //                 }
    //                 Commands::Print { message: _ } => (),
    //             },
    //             Rule::Debug { target } => match target {
    //                 Some(name) => match node.variables.get(&SmolStr::new(name)) {
    //                     Some(_) => (),
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::VariableNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 None => (),
    //             },
    //         }
    //     }

    //     pub fn validate_ruleblock(
    //         &self,
    //         ruleblock: &'a Vec<Rule<'a>>,
    //         node: &Node<'a>,
    //         lexer: &Lexer,
    //         laf: &mut LostAndFound<'a>,
    //         result: &mut ValidationResult<'a>,
    //     ) {
    //         let steps = laf.steps;
    //         for rule in ruleblock {
    //             laf.steps += 1;
    //             self.validate_rule(rule, node, lexer, laf, result);
    //         }
    //         laf.steps = steps;
    //     }

    //     pub fn validate_token(
    //         &self,
    //         token: &'a MatchToken,
    //         node: &Node<'a>,
    //         lexer: &Lexer,
    //         _laf: &mut LostAndFound,
    //         result: &mut ValidationResult<'a>,
    //     ) {
    //         match token {
    //             MatchToken::Node(name) => {
    //                 if !self.nodes.contains_key(&SmolStr::new(name)) {
    //                     result.errors.push(ValidationError {
    //                         kind: ValidationErrors::NodeNotFound(&name),
    //                         node_name: node.name.clone(),
    //                     });
    //                 }
    //             }
    //             MatchToken::Enumerator(enumerator) => {
    //                 if !self.enumerators.contains_key(&SmolStr::new(enumerator)) {
    //                     result.errors.push(ValidationError {
    //                         kind: ValidationErrors::EnumeratorNotFound(&enumerator),
    //                         node_name: node.name.clone(),
    //                     });
    //                 }
    //             }
    //             MatchToken::Any => result.warnings.push(ValidationWarning {
    //                 kind: ValidationWarnings::UsedDepricated(Depricated::Any),
    //                 node_name: node.name.clone(),
    //             }),
    //             MatchToken::Token(kind) => match kind {
    //                 TokenKinds::Token(txt) => {
    //                     if txt.is_empty() {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::EmptyToken,
    //                             node_name: node.name.clone(),
    //                         });
    //                         return;
    //                     }
    //                     // check if token is in the lexer
    //                     if !lexer.token_kinds.iter().any(|k| k == txt) {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::TokenNotFound(&txt),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 }
    //                 _ => {}
    //             },
    //             _ => {}
    //         }
    //     }

    //     pub fn validate_parameters(
    //         &self,
    //         parameters: &Vec<Parameters<'a>>,
    //         node: &Node<'a>,
    //         laf: &mut LostAndFound<'a>,
    //         result: &mut ValidationResult<'a>,
    //     ) {
    //         for parameter in parameters {
    //             match parameter {
    //                 Parameters::Set(name) => match node.variables.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Node => (),
    //                         VariableKind::NodeList => (),
    //                         VariableKind::Boolean => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Number => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::VariableNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::Global(name) => match self.globals.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Node => (),
    //                         VariableKind::NodeList => (),
    //                         VariableKind::Boolean => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Number => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::GlobalNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::Increment(name) => match node.variables.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Number => (),
    //                         VariableKind::Node => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::NodeList => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Boolean => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::VariableNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::Decrement(name) => match node.variables.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Number => (),
    //                         VariableKind::Node => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::NodeList => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Boolean => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::VariableNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::IncrementGlobal(name) => {
    //                     match self.globals.get(&SmolStr::new(name)) {
    //                         Some(var) => match var {
    //                             VariableKind::Number => (),
    //                             VariableKind::Node => result.errors.push(ValidationError {
    //                                 kind: ValidationErrors::CantUseVariable(&name),
    //                                 node_name: node.name.clone(),
    //                             }),
    //                             VariableKind::NodeList => result.errors.push(ValidationError {
    //                                 kind: ValidationErrors::CantUseVariable(&name),
    //                                 node_name: node.name.clone(),
    //                             }),
    //                             VariableKind::Boolean => result.errors.push(ValidationError {
    //                                 kind: ValidationErrors::CantUseVariable(&name),
    //                                 node_name: node.name.clone(),
    //                             }),
    //                         },
    //                         None => {
    //                             result.errors.push(ValidationError {
    //                                 kind: ValidationErrors::GlobalNotFound(&name),
    //                                 node_name: node.name.clone(),
    //                             });
    //                         }
    //                     }
    //                 }
    //                 Parameters::True(name) => match node.variables.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Boolean => (),
    //                         VariableKind::Node => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::NodeList => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Number => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::VariableNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::False(name) => match node.variables.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Boolean => (),
    //                         VariableKind::Node => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::NodeList => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Number => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::VariableNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::TrueGlobal(name) => match self.globals.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Boolean => (),
    //                         VariableKind::Node => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::NodeList => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Number => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::GlobalNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::FalseGlobal(name) => match self.globals.get(&SmolStr::new(name)) {
    //                     Some(var) => match var {
    //                         VariableKind::Boolean => (),
    //                         VariableKind::Node => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::NodeList => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                         VariableKind::Number => result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CantUseVariable(&name),
    //                             node_name: node.name.clone(),
    //                         }),
    //                     },
    //                     None => {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::GlobalNotFound(&name),
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 },
    //                 Parameters::Print(_) => {
    //                     result.warnings.push(ValidationWarning {
    //                         kind: ValidationWarnings::UsedPrint,
    //                         node_name: node.name.clone(),
    //                     });
    //                 }
    //                 Parameters::Debug(node_option) => {
    //                     match node_option {
    //                         Some(name) => match node.variables.get(&SmolStr::new(name)) {
    //                             Some(_) => (),
    //                             None => {
    //                                 result.errors.push(ValidationError {
    //                                     kind: ValidationErrors::VariableNotFound(&name),
    //                                     node_name: node.name.clone(),
    //                                 });
    //                             }
    //                         },
    //                         None => (),
    //                     }
    //                     result.warnings.push(ValidationWarning {
    //                         kind: ValidationWarnings::UsedDebug,
    //                         node_name: node.name.clone(),
    //                     });
    //                 }
    //                 Parameters::Back(n) => {
    //                     result.warnings.push(ValidationWarning {
    //                         kind: ValidationWarnings::UsedDepricated(Depricated::Back),
    //                         node_name: node.name.clone(),
    //                     });
    //                     if *n as usize > laf.steps {
    //                         result.errors.push(ValidationError {
    //                             kind: ValidationErrors::CannotGoBackMoreThan {
    //                                 steps: *n as usize,
    //                                 max: laf.steps,
    //                             },
    //                             node_name: node.name.clone(),
    //                         });
    //                     }
    //                 }
    //                 Parameters::Return => (),
    //                 Parameters::Break(_) => (),
    //                 Parameters::HardError(_) => (),
    //                 Parameters::Goto(label) => {
    //                     laf.lost_labels.push(&label);
    //                 }
    //                 Parameters::NodeStart => (),
    //                 Parameters::NodeEnd => (),
    //             }
    //         }
    //     }
    // }

    pub struct ValidationResult<'a> {
        pub errors: Vec<ValidationError<'a>>,
        pub warnings: Vec<ValidationWarning<'a>>,
    }

    impl<'a> ValidationResult<'a> {
        pub fn new() -> Self {
            Self {
                errors: Vec::new(),
                warnings: Vec::new(),
            }
        }

        /// Returns true if there are no errors and no warnings
        ///
        /// Choose this over `pass` for production code
        ///
        /// ```rust
        /// let result = grammar.validate(&lexer);
        /// if result.success() {
        ///    println!("Grammar is valid and production ready");
        /// } else {
        ///   println!("Grammar is not valid");
        /// }
        /// ```
        ///
        pub fn success(&self) -> bool {
            self.errors.is_empty() && self.warnings.is_empty()
        }

        /// Returns true if there are no errors
        ///
        /// Choose this over `success` for testing code
        ///
        /// ```rust
        /// let result = grammar.validate(&lexer);
        /// if result.pass() {
        ///   println!("Grammar is valid and good for testing");
        /// } else {
        ///  println!("Grammar is not valid");
        /// }
        /// ```
        ///
        pub fn pass(&self) -> bool {
            self.errors.is_empty()
        }
    }

    #[derive(Debug, Clone)]
    pub struct ValidationError<'a> {
        pub kind: ValidationErrors<'a>,
        pub node_name: &'a str,
    }

    #[derive(Debug, Clone)]
    pub enum ValidationErrors<'a> {
        CantUseVariable(Key<VariableTag>),
        CantUseGlobalVariable(Key<GlobalVariableTag>),
        EmptyToken,
        TokenNotFound(&'a str),
        DuplicateLabel(&'a str),
        LabelNotFound(&'a str),
        TokenCollision(SmolStr),
        CannotGoBackMoreThan { steps: usize, max: usize },
    }

    #[derive(Debug, Clone)]
    pub struct ValidationWarning<'a> {
        pub kind: ValidationWarnings<'a>,
        pub node_name: &'a str,
    }

    #[derive(Debug, Clone)]
    pub enum ValidationWarnings<'a> {
        UnusedVariable(&'a str),
        UsedDebug,
        UsedPrint,
        UsedDepricated(Depricated),
        UnusualToken(SmolStr, TokenErrors),
        UnusedLabel(&'a str),
    }

    #[derive(Deserialize, Debug, Clone)]
    pub enum TokenErrors {
        NotAscii,
        ContainsWhitespace,
        TooLong,
        StartsNumeric,
    }

    #[derive(Deserialize, Debug, Clone)]
    pub enum Depricated {
        /// The node is depricated
        ///
        /// It is advised to use Goto instead
        Back,
        /// Maybe you should use a different approach
        Any,
    }

    /// This is a structure that keeps track of things that are hard to find
    #[derive(Debug)]
    pub struct LostAndFound<'a> {
        pub lost_labels: Vec<&'a str>,
        pub found_labels: Vec<&'a str>,
        /// The maximum number of steps that can be taken back
        pub steps: usize,
    }

    impl<'a> LostAndFound<'a> {
        pub fn new() -> Self {
            Self {
                lost_labels: Vec::new(),
                found_labels: Vec::new(),
                steps: 0,
            }
        }

        pub fn pass(&self, result: &mut ValidationResult<'a>, node_name: &'a str) {
            for looking_for in &self.lost_labels {
                if !self.found_labels.contains(looking_for) {
                    result.errors.push(ValidationError {
                        kind: ValidationErrors::LabelNotFound(looking_for.clone()),
                        node_name: node_name,
                    });
                }
            }
            for found in &self.found_labels {
                if !self.lost_labels.contains(found) {
                    result.warnings.push(ValidationWarning {
                        kind: ValidationWarnings::UnusedLabel(found),
                        node_name: node_name,
                    });
                }
            }
        }
    }
}
