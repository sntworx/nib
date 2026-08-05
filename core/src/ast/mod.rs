mod parser;
pub(crate) mod types;

pub use parser::ParseError;

use parser::Parser;
use types::AstNode;

use crate::lexer::Token;

#[derive(Debug)]
pub struct Ast {
    nodes: Vec<AstNode>,
}

impl Ast {
    pub fn parse(tokens: Vec<Token>) -> Result<Ast, ParseError> {
        Parser::new(tokens).parse()
    }

    pub fn nodes(&self) -> &[AstNode] {
        &self.nodes
    }

    pub(crate) fn from_nodes(nodes: Vec<AstNode>) -> Ast {
        Ast { nodes }
    }
}
