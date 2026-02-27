//! Manchester Syntax Grammar Rules
//!
//! This module defines the grammar rules and production handling
//! for Manchester Syntax parsing.

use super::tokenizer::{Token, TokenType};

/// Grammar production types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Production {
    /// Start symbol
    Document,

    /// Declarations
    PrefixDeclaration,
    ClassDeclaration,
    ObjectPropertyDeclaration,
    DataPropertyDeclaration,
    IndividualDeclaration,
    AnnotationDeclaration,

    /// Expressions
    ClassExpression,
    ObjectPropertyExpression,
    DataPropertyExpression,
    DataRange,
    IndividualExpression,

    /// Logical expressions
    ObjectIntersectionOf,
    ObjectUnionOf,
    ObjectComplementOf,
    DataIntersectionOf,
    DataUnionOf,
    DataComplementOf,

    /// Restrictions
    ObjectSomeValuesFrom,
    ObjectAllValuesFrom,
    ObjectHasValue,
    ObjectHasSelf,
    ObjectCardinality,
    DataSomeValuesFrom,
    DataAllValuesFrom,
    DataHasValue,
    DataCardinality,

    /// Property characteristics
    PropertyCharacteristicList,
    TransitiveProperty,
    AsymmetricProperty,
    IrreflexiveProperty,
    FunctionalProperty,
    InverseFunctionalProperty,

    /// Property assertions
    PropertyAssertion,
    NegativePropertyAssertion,

    /// Annotations
    Annotation,
    AnnotationAssertion,

    /// Rules (SWRL)
    Rule,
    RuleBody,
    RuleHead,
}

/// Grammar rule with precedence and associativity
#[derive(Debug, Clone)]
pub struct GrammarRule {
    /// The non-terminal this rule produces
    pub production: Production,

    /// The sequence of symbols (terminals and non-terminals) that form this rule
    pub symbols: Vec<Symbol>,

    /// The precedence level of this rule
    pub precedence: u32,

    /// Whether this rule is left-associative
    pub left_associative: bool,

    /// Semantic action to execute when this rule is reduced
    pub action: Option<fn(Vec<AstNode>) -> AstNode>,
}

/// Grammar symbol (terminal or non-terminal)
#[derive(Debug, Clone, PartialEq)]
pub enum Symbol {
    /// Terminal symbol (token)
    Terminal(TokenType),

    /// Non-terminal symbol
    NonTerminal(Production),

    /// Epsilon (empty)
    Epsilon,
}

/// Abstract Syntax Tree node for grammar parsing
#[derive(Debug, Clone)]
pub enum AstNode {
    /// Token node
    Token(Token),

    /// Production node
    Production(Production, Vec<AstNode>),

    /// Error node
    Error(String),
}

/// Manchester Syntax Grammar
pub struct ManchesterGrammar {
    /// All grammar rules
    rules: Vec<GrammarRule>,

    /// Start symbol
    start_symbol: Production,

    /// Precedence levels for operators
    precedence_levels: std::collections::HashMap<String, u32>,
}

impl Default for ManchesterGrammar {
    fn default() -> Self {
        Self::new()
    }
}

impl ManchesterGrammar {
    /// Create a new Manchester grammar
    pub fn new() -> Self {
        let mut grammar = ManchesterGrammar {
            rules: Vec::new(),
            start_symbol: Production::Document,
            precedence_levels: std::collections::HashMap::new(),
        };

        grammar.setup_precedence();
        grammar.setup_rules();
        grammar
    }

    /// Setup operator precedence levels
    fn setup_precedence(&mut self) {
        // Logical operators (highest precedence)
        self.precedence_levels.insert("not".to_string(), 100);
        self.precedence_levels.insert("and".to_string(), 90);
        self.precedence_levels.insert("or".to_string(), 80);

        // Restriction operators
        self.precedence_levels.insert("some".to_string(), 70);
        self.precedence_levels.insert("only".to_string(), 70);
        self.precedence_levels.insert("value".to_string(), 70);
        self.precedence_levels.insert("Self".to_string(), 70);

        // Cardinality operators
        self.precedence_levels.insert("min".to_string(), 60);
        self.precedence_levels.insert("max".to_string(), 60);
        self.precedence_levels.insert("exactly".to_string(), 60);

        // Property operators
        self.precedence_levels.insert("inverse".to_string(), 50);
    }

    /// Setup grammar rules
    fn setup_rules(&mut self) {
        // Document rule
        self.add_rule(GrammarRule {
            production: Production::Document,
            symbols: vec![Symbol::NonTerminal(Production::PrefixDeclaration)],
            precedence: 0,
            left_associative: false,
            action: Some(|nodes| nodes[0].clone()),
        });

        self.add_rule(GrammarRule {
            production: Production::Document,
            symbols: vec![Symbol::NonTerminal(Production::ClassDeclaration)],
            precedence: 0,
            left_associative: false,
            action: Some(|nodes| nodes[0].clone()),
        });

        // Prefix declaration
        self.add_rule(GrammarRule {
            production: Production::PrefixDeclaration,
            symbols: vec![
                Symbol::Terminal(TokenType::Prefix),
                Symbol::Terminal(TokenType::Colon),
                Symbol::Terminal(TokenType::Identifier),
                Symbol::Terminal(TokenType::IRI),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_prefix_declaration),
        });

        // Class declaration
        self.add_rule(GrammarRule {
            production: Production::ClassDeclaration,
            symbols: vec![
                Symbol::Terminal(TokenType::Class),
                Symbol::Terminal(TokenType::Colon),
                Symbol::Terminal(TokenType::Identifier),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_class_declaration),
        });

        // Class declaration with subclass
        self.add_rule(GrammarRule {
            production: Production::ClassDeclaration,
            symbols: vec![
                Symbol::NonTerminal(Production::ClassDeclaration),
                Symbol::Terminal(TokenType::SubClassOf),
                Symbol::NonTerminal(Production::ClassExpression),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::add_subclass_to_class),
        });

        // Simple class expression
        self.add_rule(GrammarRule {
            production: Production::ClassExpression,
            symbols: vec![Symbol::Terminal(TokenType::Identifier)],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_named_class),
        });

        // Intersection expression
        self.add_rule(GrammarRule {
            production: Production::ClassExpression,
            symbols: vec![
                Symbol::Terminal(TokenType::LeftParen),
                Symbol::Terminal(TokenType::Identifier), // "and"
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::Terminal(TokenType::RightParen),
            ],
            precedence: 90,
            left_associative: true,
            action: Some(Self::build_intersection),
        });

        // Union expression
        self.add_rule(GrammarRule {
            production: Production::ClassExpression,
            symbols: vec![
                Symbol::Terminal(TokenType::LeftParen),
                Symbol::Terminal(TokenType::Identifier), // "or"
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::Terminal(TokenType::RightParen),
            ],
            precedence: 80,
            left_associative: true,
            action: Some(Self::build_union),
        });

        // Complement expression
        self.add_rule(GrammarRule {
            production: Production::ClassExpression,
            symbols: vec![
                Symbol::Terminal(TokenType::LeftParen),
                Symbol::Terminal(TokenType::Identifier), // "not"
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::Terminal(TokenType::RightParen),
            ],
            precedence: 100,
            left_associative: false,
            action: Some(Self::build_complement),
        });

        // Object property expression
        self.add_rule(GrammarRule {
            production: Production::ObjectPropertyExpression,
            symbols: vec![Symbol::Terminal(TokenType::Identifier)],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_named_object_property),
        });

        // Inverse property expression
        self.add_rule(GrammarRule {
            production: Production::ObjectPropertyExpression,
            symbols: vec![
                Symbol::Terminal(TokenType::Inverse),
                Symbol::Terminal(TokenType::LeftParen),
                Symbol::NonTerminal(Production::ObjectPropertyExpression),
                Symbol::Terminal(TokenType::RightParen),
            ],
            precedence: 50,
            left_associative: false,
            action: Some(Self::build_inverse_property),
        });

        // Some values from restriction
        self.add_rule(GrammarRule {
            production: Production::ClassExpression,
            symbols: vec![
                Symbol::Terminal(TokenType::LeftParen),
                Symbol::NonTerminal(Production::ObjectPropertyExpression),
                Symbol::Terminal(TokenType::Some),
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::Terminal(TokenType::RightParen),
            ],
            precedence: 70,
            left_associative: false,
            action: Some(Self::build_some_values_from),
        });

        // All values from restriction
        self.add_rule(GrammarRule {
            production: Production::ClassExpression,
            symbols: vec![
                Symbol::Terminal(TokenType::LeftParen),
                Symbol::NonTerminal(Production::ObjectPropertyExpression),
                Symbol::Terminal(TokenType::Only),
                Symbol::NonTerminal(Production::ClassExpression),
                Symbol::Terminal(TokenType::RightParen),
            ],
            precedence: 70,
            left_associative: false,
            action: Some(Self::build_all_values_from),
        });

        // Object property declaration with characteristics
        self.add_rule(GrammarRule {
            production: Production::ObjectPropertyDeclaration,
            symbols: vec![
                Symbol::Terminal(TokenType::ObjectProperty),
                Symbol::Terminal(TokenType::Colon),
                Symbol::Terminal(TokenType::Identifier),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_object_property_declaration),
        });

        // Object property with characteristics
        self.add_rule(GrammarRule {
            production: Production::ObjectPropertyDeclaration,
            symbols: vec![
                Symbol::NonTerminal(Production::ObjectPropertyDeclaration),
                Symbol::Terminal(TokenType::Characteristics),
                Symbol::Terminal(TokenType::Colon),
                Symbol::NonTerminal(Production::PropertyCharacteristicList),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::add_characteristics_to_property),
        });

        // Property characteristic list
        self.add_rule(GrammarRule {
            production: Production::PropertyCharacteristicList,
            symbols: vec![Symbol::Terminal(TokenType::Functional)],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_characteristic_list),
        });

        self.add_rule(GrammarRule {
            production: Production::PropertyCharacteristicList,
            symbols: vec![Symbol::Terminal(TokenType::Transitive)],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_characteristic_list),
        });

        self.add_rule(GrammarRule {
            production: Production::PropertyCharacteristicList,
            symbols: vec![Symbol::Terminal(TokenType::Asymmetric)],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_characteristic_list),
        });

        self.add_rule(GrammarRule {
            production: Production::PropertyCharacteristicList,
            symbols: vec![Symbol::Terminal(TokenType::Irreflexive)],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_characteristic_list),
        });

        // Multiple characteristics
        self.add_rule(GrammarRule {
            production: Production::PropertyCharacteristicList,
            symbols: vec![
                Symbol::NonTerminal(Production::PropertyCharacteristicList),
                Symbol::Terminal(TokenType::Comma),
                Symbol::NonTerminal(Production::PropertyCharacteristicList),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::build_multiple_characteristics),
        });

        // Object property with InverseOf
        self.add_rule(GrammarRule {
            production: Production::ObjectPropertyDeclaration,
            symbols: vec![
                Symbol::NonTerminal(Production::ObjectPropertyDeclaration),
                Symbol::Terminal(TokenType::InverseOf),
                Symbol::Terminal(TokenType::Colon),
                Symbol::Terminal(TokenType::Identifier),
            ],
            precedence: 0,
            left_associative: false,
            action: Some(Self::add_inverse_to_property),
        });
    }

    /// Add a grammar rule
    fn add_rule(&mut self, rule: GrammarRule) {
        self.rules.push(rule);
    }

    /// Get all rules for a production
    pub fn get_rules_for_production(&self, production: &Production) -> Vec<&GrammarRule> {
        self.rules
            .iter()
            .filter(|rule| rule.production == *production)
            .collect()
    }

    /// Get the start symbol
    pub fn start_symbol(&self) -> &Production {
        &self.start_symbol
    }

    /// Get precedence for an operator
    pub fn get_precedence(&self, operator: &str) -> Option<u32> {
        self.precedence_levels.get(operator).copied()
    }

    // Semantic action functions

    fn build_prefix_declaration(nodes: Vec<AstNode>) -> AstNode {
        if let (Some(AstNode::Token(_prefix_token)), Some(AstNode::Token(_iri_token))) =
            (nodes.get(2), nodes.get(3))
        {
            // This would build a proper AST node in a real implementation
            AstNode::Production(Production::PrefixDeclaration, nodes)
        } else {
            AstNode::Error("Invalid prefix declaration".to_string())
        }
    }

    fn build_class_declaration(nodes: Vec<AstNode>) -> AstNode {
        if let Some(AstNode::Token(_name_token)) = nodes.get(2) {
            // This would build a proper class declaration AST node
            AstNode::Production(Production::ClassDeclaration, nodes)
        } else {
            AstNode::Error("Invalid class declaration".to_string())
        }
    }

    fn add_subclass_to_class(nodes: Vec<AstNode>) -> AstNode {
        // This would add a subclass relationship to an existing class declaration
        AstNode::Production(Production::ClassDeclaration, nodes)
    }

    fn build_named_class(nodes: Vec<AstNode>) -> AstNode {
        if let Some(AstNode::Token(_name_token)) = nodes.first() {
            AstNode::Production(Production::ClassExpression, nodes)
        } else {
            AstNode::Error("Invalid named class".to_string())
        }
    }

    fn build_intersection(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectIntersectionOf, nodes)
    }

    fn build_union(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectUnionOf, nodes)
    }

    fn build_complement(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectComplementOf, nodes)
    }

    fn build_named_object_property(nodes: Vec<AstNode>) -> AstNode {
        if let Some(AstNode::Token(_name_token)) = nodes.first() {
            AstNode::Production(Production::ObjectPropertyExpression, nodes)
        } else {
            AstNode::Error("Invalid named object property".to_string())
        }
    }

    fn build_inverse_property(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectPropertyExpression, nodes)
    }

    fn build_some_values_from(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectSomeValuesFrom, nodes)
    }

    fn build_all_values_from(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectAllValuesFrom, nodes)
    }

    // Action functions for property characteristics

    fn build_object_property_declaration(nodes: Vec<AstNode>) -> AstNode {
        if let Some(AstNode::Token(_name_token)) = nodes.get(2) {
            AstNode::Production(Production::ObjectPropertyDeclaration, nodes)
        } else {
            AstNode::Error("Invalid object property declaration".to_string())
        }
    }

    fn add_characteristics_to_property(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectPropertyDeclaration, nodes)
    }

    fn build_characteristic_list(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::PropertyCharacteristicList, nodes)
    }

    fn build_multiple_characteristics(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::PropertyCharacteristicList, nodes)
    }

    fn add_inverse_to_property(nodes: Vec<AstNode>) -> AstNode {
        AstNode::Production(Production::ObjectPropertyDeclaration, nodes)
    }

    /// Validate that the grammar is properly formed
    pub fn validate(&self) -> Result<(), String> {
        // Check that all productions have at least one rule
        let required_productions = vec![
            Production::Document,
            Production::PrefixDeclaration,
            Production::ClassDeclaration,
            Production::ClassExpression,
            Production::ObjectPropertyExpression,
        ];

        for required in required_productions {
            let mut found = false;
            for rule in &self.rules {
                if rule.production == required {
                    found = true;
                    break;
                }
            }
            if !found {
                return Err(format!("Missing rules for production: {:?}", required));
            }
        }

        // Check that precedence levels are properly ordered
        let mut precedences: Vec<u32> = self.precedence_levels.values().cloned().collect();
        precedences.sort_unstable();
        precedences.dedup();

        for window in precedences.windows(2) {
            if window[0] == window[1] {
                return Err("Duplicate precedence levels detected".to_string());
            }
        }

        Ok(())
    }

    /// Get grammar statistics
    pub fn statistics(&self) -> GrammarStatistics {
        let _production_counts: std::collections::HashMap<Production, usize> =
            std::collections::HashMap::new();
        let _terminal_counts: std::collections::HashMap<TokenType, usize> =
            std::collections::HashMap::new();

        GrammarStatistics {
            total_rules: self.rules.len(),
            total_productions: self
                .rules
                .iter()
                .map(|r| std::mem::discriminant(&r.production))
                .collect::<std::collections::HashSet<_>>()
                .len(),
            max_rule_length: self
                .rules
                .iter()
                .map(|r| r.symbols.len())
                .max()
                .unwrap_or(0),
            average_rule_length: self.rules.iter().map(|r| r.symbols.len()).sum::<usize>() as f64
                / self.rules.len() as f64,
        }
    }
}

/// Grammar statistics
#[derive(Debug, Clone)]
pub struct GrammarStatistics {
    pub total_rules: usize,
    pub total_productions: usize,
    pub max_rule_length: usize,
    pub average_rule_length: f64,
}

/// Parser state for LR parsing
#[derive(Debug, Clone)]
pub struct ParserState {
    /// Current state number
    pub state: usize,

    /// Stack of symbols and states
    pub stack: Vec<(Symbol, usize)>,

    /// Current position in input
    pub position: usize,

    /// Whether we're in error recovery mode
    pub error_recovery: bool,
}

impl ParserState {
    /// Create a new parser state
    pub fn new() -> Self {
        ParserState {
            state: 0,
            stack: vec![(Symbol::Epsilon, 0)], // Start with initial state
            position: 0,
            error_recovery: false,
        }
    }

    /// Push a symbol onto the stack
    pub fn push(&mut self, symbol: Symbol, state: usize) {
        self.stack.push((symbol, state));
    }

    /// Pop a symbol from the stack
    pub fn pop(&mut self) -> Option<(Symbol, usize)> {
        self.stack.pop()
    }

    /// Get the top symbol without popping
    pub fn top(&self) -> Option<&(Symbol, usize)> {
        self.stack.last()
    }

    /// Clear the stack
    pub fn clear(&mut self) {
        self.stack.clear();
    }

    /// Get the current stack depth
    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

impl Default for ParserState {
    fn default() -> Self {
        Self::new()
    }
}
