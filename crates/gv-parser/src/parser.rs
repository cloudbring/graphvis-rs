//! Recursive-descent DOT parser matching Graphviz `lib/cgraph/grammar.y`,
//! with full lexical scoping for `graph`, `node`, and `edge` defaults across
//! nested subgraphs and clusters.

use crate::lexer::{Lexer, Token, TokenKind};
use gv_core::{
    AttrMap, BoundingBox, EdgeId, Graph, GraphKind, GraphvizError, NodeId, Port, Result,
    SubgraphData, SubgraphId,
};

const MAX_SUBGRAPH_DEPTH: usize = 256;

#[derive(Debug, Clone)]
enum SimpleItem {
    Nodes(Vec<(NodeId, Port)>),
    Subgraph(Vec<(NodeId, Port)>),
}

impl SimpleItem {
    fn into_endpoints(self) -> Vec<(NodeId, Port)> {
        match self {
            Self::Nodes(v) | Self::Subgraph(v) => v,
        }
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    graph: Graph,
    scope_stack: Vec<SubgraphId>,
}

impl Parser {
    pub fn parse_str(input: &str) -> Result<Graph> {
        let mut lexer = Lexer::new(input);
        let tokens = lexer.tokenize_all()?;
        let mut parser = Self {
            tokens,
            pos: 0,
            graph: Graph::new("", GraphKind::Undirected, false),
            scope_stack: Vec::new(),
        };
        parser.parse_graph()?;
        Ok(parser.graph)
    }

    fn peek(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("tokens always ends with Eof"))
    }

    fn peek_n(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.pos + offset)
            .unwrap_or_else(|| self.tokens.last().expect("tokens always ends with Eof"))
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token> {
        let tok = self.peek().clone();
        if &tok.kind == expected {
            Ok(self.advance())
        } else {
            Err(GraphvizError::SyntaxError {
                line: tok.line,
                col: tok.col,
                message: format!("expected {:?}, found {:?}", expected, tok.kind),
            })
        }
    }

    fn is_atom_token(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Ident(_) | TokenKind::Quoted(_) | TokenKind::Html(_)
        )
    }

    fn parse_atom(&mut self) -> Result<String> {
        let tok = self.peek().clone();
        let mut val = match tok.kind {
            TokenKind::Ident(s) | TokenKind::Quoted(s) | TokenKind::Html(s) => {
                self.advance();
                s
            }
            _ => {
                return Err(GraphvizError::SyntaxError {
                    line: tok.line,
                    col: tok.col,
                    message: format!(
                        "expected identifier or string literal, found {:?}",
                        tok.kind
                    ),
                });
            }
        };

        // Support string concatenation with `+` (`qatom '+' T_qatom` in grammar.y)
        while self.peek().kind == TokenKind::Plus {
            self.advance();
            let next_tok = self.peek().clone();
            match next_tok.kind {
                TokenKind::Ident(s) | TokenKind::Quoted(s) | TokenKind::Html(s) => {
                    self.advance();
                    val.push_str(&s);
                }
                _ => {
                    return Err(GraphvizError::SyntaxError {
                        line: next_tok.line,
                        col: next_tok.col,
                        message: format!(
                            "expected string literal after '+', found {:?}",
                            next_tok.kind
                        ),
                    });
                }
            }
        }

        Ok(val)
    }

    fn parse_value_or_port_atom(&mut self) -> Result<String> {
        if Self::is_atom_token(&self.peek().kind) {
            return self.parse_atom();
        }
        let tok = self.peek().clone();
        let kw = match tok.kind {
            TokenKind::Node => Some("node"),
            TokenKind::Edge => Some("edge"),
            TokenKind::Graph => Some("graph"),
            TokenKind::Digraph => Some("digraph"),
            TokenKind::Subgraph => Some("subgraph"),
            TokenKind::Strict => Some("strict"),
            _ => None,
        };
        if let Some(s) = kw {
            self.advance();
            Ok(s.to_string())
        } else {
            self.parse_atom()
        }
    }

    /// Checks how many tokens an atom (including `+` concatenations) spans starting at `start_offset`,
    /// so we can look ahead for `=` without mutating parser state.
    fn atom_span_len(&self, start_offset: usize) -> Option<usize> {
        if !Self::is_atom_token(&self.peek_n(start_offset).kind) {
            return None;
        }
        let mut len = 1;
        while self.peek_n(start_offset + len).kind == TokenKind::Plus {
            if Self::is_atom_token(&self.peek_n(start_offset + len + 1).kind) {
                len += 2;
            } else {
                break;
            }
        }
        Some(len)
    }

    fn parse_graph(&mut self) -> Result<()> {
        let strict = if self.peek().kind == TokenKind::Strict {
            self.advance();
            true
        } else {
            false
        };

        let tok = self.peek().clone();
        let kind = match tok.kind {
            TokenKind::Graph => {
                self.advance();
                GraphKind::Undirected
            }
            TokenKind::Digraph => {
                self.advance();
                GraphKind::Directed
            }
            _ => {
                return Err(GraphvizError::SyntaxError {
                    line: tok.line,
                    col: tok.col,
                    message: format!("expected 'graph' or 'digraph', found {:?}", tok.kind),
                });
            }
        };

        let name = if Self::is_atom_token(&self.peek().kind) {
            self.parse_atom()?
        } else {
            String::new()
        };

        self.graph = Graph::new(name, kind, strict);

        self.expect(&TokenKind::LBrace)?;
        self.parse_stmt_list()?;
        self.expect(&TokenKind::RBrace)?;

        // Consume optional trailing semicolons
        while self.peek().kind == TokenKind::Semi {
            self.advance();
        }

        if self.peek().kind != TokenKind::Eof {
            let extra = self.peek();
            return Err(GraphvizError::SyntaxError {
                line: extra.line,
                col: extra.col,
                message: format!("unexpected token after end of graph: {:?}", extra.kind),
            });
        }

        Ok(())
    }

    fn parse_stmt_list(&mut self) -> Result<()> {
        while self.peek().kind != TokenKind::RBrace && self.peek().kind != TokenKind::Eof {
            if self.peek().kind == TokenKind::Semi {
                self.advance();
                continue;
            }
            self.parse_stmt()?;
            if self.peek().kind == TokenKind::Semi {
                self.advance();
            }
        }
        Ok(())
    }

    fn parse_stmt(&mut self) -> Result<()> {
        match self.peek().kind {
            TokenKind::Graph | TokenKind::Node | TokenKind::Edge => self.parse_attr_stmt(),
            TokenKind::Subgraph | TokenKind::LBrace => self.parse_compound(),
            _ if Self::is_atom_token(&self.peek().kind) => {
                if let Some(span) = self.atom_span_len(0)
                    && self.peek_n(span).kind == TokenKind::Equals
                {
                    let key = self.parse_atom()?;
                    self.expect(&TokenKind::Equals)?;
                    let val = self.parse_value_or_port_atom()?;
                    self.current_graph_attrs_mut().insert(key, val);
                    return Ok(());
                }
                self.parse_compound()
            }
            _ => {
                let tok = self.peek();
                Err(GraphvizError::SyntaxError {
                    line: tok.line,
                    col: tok.col,
                    message: format!("unexpected token in statement: {:?}", tok.kind),
                })
            }
        }
    }

    fn parse_attr_stmt(&mut self) -> Result<()> {
        let attr_tok = self.advance();
        // Optional macro name `atom '='` before `[` (`optmacroname` in grammar.y)
        if let Some(span) = self.atom_span_len(0)
            && self.peek_n(span).kind == TokenKind::Equals
        {
            let _macro_name = self.parse_atom()?;
            self.expect(&TokenKind::Equals)?;
        }

        if self.peek().kind != TokenKind::LBracket {
            let tok = self.peek();
            return Err(GraphvizError::SyntaxError {
                line: tok.line,
                col: tok.col,
                message: format!(
                    "expected '[' after {:?} attribute statement, found {:?}",
                    attr_tok.kind, tok.kind
                ),
            });
        }

        let attrs = self.parse_opt_attr_list()?;
        match attr_tok.kind {
            TokenKind::Graph => self.current_graph_attrs_mut().extend(attrs),
            TokenKind::Node => self.current_node_defaults_mut().extend(attrs),
            TokenKind::Edge => self.current_edge_defaults_mut().extend(attrs),
            _ => unreachable!(),
        }
        Ok(())
    }

    fn parse_opt_attr_list(&mut self) -> Result<AttrMap> {
        let mut attrs = AttrMap::new();
        while self.peek().kind == TokenKind::LBracket {
            self.advance(); // consume `[`
            while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                if matches!(self.peek().kind, TokenKind::Comma | TokenKind::Semi) {
                    self.advance();
                    continue;
                }
                let key = self.parse_value_or_port_atom()?;
                let val = if self.peek().kind == TokenKind::Equals {
                    self.advance();
                    self.parse_value_or_port_atom()?
                } else {
                    "true".to_string()
                };
                attrs.insert(key, val);
                if matches!(self.peek().kind, TokenKind::Comma | TokenKind::Semi) {
                    self.advance();
                }
            }
            self.expect(&TokenKind::RBracket)?;
        }
        Ok(attrs)
    }

    fn parse_compound(&mut self) -> Result<()> {
        let first = self.parse_simple()?;

        if matches!(self.peek().kind, TokenKind::Arrow | TokenKind::DashDash) {
            let mut chain = vec![first.into_endpoints()];
            while matches!(self.peek().kind, TokenKind::Arrow | TokenKind::DashDash) {
                let op_tok = self.advance();
                match (op_tok.kind, self.graph.kind) {
                    (TokenKind::Arrow, GraphKind::Undirected) => {
                        return Err(GraphvizError::SyntaxError {
                            line: op_tok.line,
                            col: op_tok.col,
                            message:
                                "directed edge operator '->' is not allowed in undirected 'graph'"
                                    .to_string(),
                        });
                    }
                    (TokenKind::DashDash, GraphKind::Directed) => {
                        return Err(GraphvizError::SyntaxError {
                            line: op_tok.line,
                            col: op_tok.col,
                            message:
                                "undirected edge operator '--' is not allowed in directed 'digraph'"
                                    .to_string(),
                        });
                    }
                    _ => {}
                }
                let next_item = self.parse_simple()?;
                chain.push(next_item.into_endpoints());
            }

            let edge_attrs = self.parse_opt_attr_list()?;
            for pair in chain.windows(2) {
                let tails = &pair[0];
                let heads = &pair[1];
                for (tail_id, tail_port) in tails {
                    for (head_id, head_port) in heads {
                        self.create_edge(
                            *tail_id,
                            tail_port.clone(),
                            *head_id,
                            head_port.clone(),
                            &edge_attrs,
                        );
                    }
                }
            }
        } else {
            let attrs = self.parse_opt_attr_list()?;
            if let SimpleItem::Nodes(nodes) = first
                && !attrs.is_empty()
            {
                for (node_id, _port) in nodes {
                    self.graph.nodes[node_id.0 as usize]
                        .attrs
                        .extend(attrs.clone());
                }
            }
        }

        Ok(())
    }

    fn parse_simple(&mut self) -> Result<SimpleItem> {
        if matches!(self.peek().kind, TokenKind::Subgraph | TokenKind::LBrace) {
            let subg_id = self.parse_subgraph()?;
            let endpoints = self.graph.subgraphs[subg_id.0 as usize]
                .nodes
                .iter()
                .map(|&nid| (nid, Port::default()))
                .collect();
            Ok(SimpleItem::Subgraph(endpoints))
        } else {
            let mut nodes = vec![self.parse_node_ref()?];
            while self.peek().kind == TokenKind::Comma {
                self.advance();
                nodes.push(self.parse_node_ref()?);
            }
            Ok(SimpleItem::Nodes(nodes))
        }
    }

    fn parse_node_ref(&mut self) -> Result<(NodeId, Port)> {
        let name = self.parse_atom()?;
        let mut port = Port::default();
        if self.peek().kind == TokenKind::Colon {
            self.advance();
            let p1 = self.parse_value_or_port_atom()?;
            if self.peek().kind == TokenKind::Colon {
                self.advance();
                let p2 = self.parse_value_or_port_atom()?;
                port.name = Some(p1);
                port.compass = Some(p2);
            } else {
                port.name = Some(p1);
            }
        }
        let node_id = self.ensure_scoped_node(&name);
        Ok((node_id, port))
    }

    fn parse_subgraph(&mut self) -> Result<SubgraphId> {
        let name = if self.peek().kind == TokenKind::Subgraph {
            self.advance();
            if Self::is_atom_token(&self.peek().kind) {
                self.parse_atom()?
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        if self.scope_stack.len() >= MAX_SUBGRAPH_DEPTH {
            let tok = self.peek();
            return Err(GraphvizError::SyntaxError {
                line: tok.line,
                col: tok.col,
                message: format!("subgraphs nested more than {MAX_SUBGRAPH_DEPTH} deep"),
            });
        }

        let subg_id = self.open_subgraph(name.clone());

        if self.peek().kind == TokenKind::LBrace {
            self.scope_stack.push(subg_id);
            self.advance(); // consume `{`
            self.parse_stmt_list()?;
            self.expect(&TokenKind::RBrace)?;
            self.scope_stack.pop();
        } else if name.is_empty() {
            let tok = self.peek();
            return Err(GraphvizError::SyntaxError {
                line: tok.line,
                col: tok.col,
                message: format!("expected '{{' for anonymous subgraph, found {:?}", tok.kind),
            });
        }

        Ok(subg_id)
    }

    fn open_subgraph(&mut self, name: String) -> SubgraphId {
        if !name.is_empty()
            && let Some(existing) = self.graph.subgraphs.iter().find(|s| s.name == name)
        {
            return existing.id;
        }

        let id = SubgraphId(self.graph.subgraphs.len() as u32);
        let is_cluster = name.to_ascii_lowercase().starts_with("cluster");
        let graph_attrs = self.current_graph_attrs().clone();
        let node_defaults = self.current_node_defaults().clone();
        let edge_defaults = self.current_edge_defaults().clone();

        if let Some(&parent_id) = self.scope_stack.last() {
            let parent_subgraphs = &mut self.graph.subgraphs[parent_id.0 as usize].subgraphs;
            if !parent_subgraphs.contains(&id) {
                parent_subgraphs.push(id);
            }
        }

        self.graph.subgraphs.push(SubgraphData {
            id,
            name,
            is_cluster,
            graph_attrs,
            node_defaults,
            edge_defaults,
            nodes: Vec::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            bb: BoundingBox::default(),
        });

        id
    }

    fn ensure_scoped_node(&mut self, name: &str) -> NodeId {
        let is_new = !self.graph.node_lookup.contains_key(name);
        let node_id = self.graph.ensure_node(name);
        if is_new {
            let mut attrs = self.current_node_defaults().clone();
            attrs
                .entry("label".to_string())
                .or_insert_with(|| "\\N".to_string());
            self.graph.nodes[node_id.0 as usize].attrs = attrs;
        }
        self.record_node_in_active_subgraphs(node_id);
        node_id
    }

    fn create_edge(
        &mut self,
        tail: NodeId,
        mut tail_port: Port,
        head: NodeId,
        mut head_port: Port,
        edge_attrs: &AttrMap,
    ) -> EdgeId {
        let mut merged_attrs = self.current_edge_defaults().clone();
        merged_attrs.extend(edge_attrs.clone());

        if tail_port == Port::default()
            && let Some(tp) = merged_attrs.get("tailport")
        {
            tail_port = parse_port_str(tp);
        }
        if head_port == Port::default()
            && let Some(hp) = merged_attrs.get("headport")
        {
            head_port = parse_port_str(hp);
        }

        let edge_id = self.graph.add_edge(tail, head, merged_attrs.clone());
        if let Some(edge) = self.graph.edges.get_mut(edge_id.0 as usize) {
            if tail_port != Port::default() {
                edge.tail_port = tail_port;
            }
            if head_port != Port::default() {
                edge.head_port = head_port;
            }
            edge.attrs.extend(merged_attrs);
        }

        self.record_node_in_active_subgraphs(tail);
        self.record_node_in_active_subgraphs(head);
        self.record_edge_in_active_subgraphs(edge_id);
        edge_id
    }

    fn record_node_in_active_subgraphs(&mut self, node_id: NodeId) {
        for &subg_id in &self.scope_stack {
            let nodes = &mut self.graph.subgraphs[subg_id.0 as usize].nodes;
            if !nodes.contains(&node_id) {
                nodes.push(node_id);
            }
        }
    }

    fn record_edge_in_active_subgraphs(&mut self, edge_id: EdgeId) {
        for &subg_id in &self.scope_stack {
            let edges = &mut self.graph.subgraphs[subg_id.0 as usize].edges;
            if !edges.contains(&edge_id) {
                edges.push(edge_id);
            }
        }
    }

    fn current_graph_attrs(&self) -> &AttrMap {
        if let Some(&subg_id) = self.scope_stack.last() {
            &self.graph.subgraphs[subg_id.0 as usize].graph_attrs
        } else {
            &self.graph.graph_attrs
        }
    }

    fn current_graph_attrs_mut(&mut self) -> &mut AttrMap {
        if let Some(&subg_id) = self.scope_stack.last() {
            &mut self.graph.subgraphs[subg_id.0 as usize].graph_attrs
        } else {
            &mut self.graph.graph_attrs
        }
    }

    fn current_node_defaults(&self) -> &AttrMap {
        if let Some(&subg_id) = self.scope_stack.last() {
            &self.graph.subgraphs[subg_id.0 as usize].node_defaults
        } else {
            &self.graph.node_defaults
        }
    }

    fn current_node_defaults_mut(&mut self) -> &mut AttrMap {
        if let Some(&subg_id) = self.scope_stack.last() {
            &mut self.graph.subgraphs[subg_id.0 as usize].node_defaults
        } else {
            &mut self.graph.node_defaults
        }
    }

    fn current_edge_defaults(&self) -> &AttrMap {
        if let Some(&subg_id) = self.scope_stack.last() {
            &self.graph.subgraphs[subg_id.0 as usize].edge_defaults
        } else {
            &self.graph.edge_defaults
        }
    }

    fn current_edge_defaults_mut(&mut self) -> &mut AttrMap {
        if let Some(&subg_id) = self.scope_stack.last() {
            &mut self.graph.subgraphs[subg_id.0 as usize].edge_defaults
        } else {
            &mut self.graph.edge_defaults
        }
    }
}

fn parse_port_str(s: &str) -> Port {
    if s.is_empty() {
        return Port::default();
    }
    if let Some((p1, p2)) = s.split_once(':') {
        Port {
            name: Some(p1.to_string()),
            compass: Some(p2.to_string()),
        }
    } else {
        Port {
            name: Some(s.to_string()),
            compass: None,
        }
    }
}
