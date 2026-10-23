//! Recursive-descent DOT parser matching Graphviz `lib/cgraph/grammar.y`,
//! with full lexical scoping for `graph`, `node`, and `edge` defaults across
//! nested subgraphs and clusters.

use crate::lexer::{Lexer, ScanInput, ScanState, Token, TokenKind};
type AttrMap2 = std::collections::BTreeMap<String, graphviz_pure_core::AttrSym>;

use graphviz_pure_core::{
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

/// Attributes declared before the graph body, as set by the command line flags
/// `-Gname=value`, `-Nname=value` and `-Ename=value`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalAttrs {
    pub graph: AttrMap,
    pub node: AttrMap,
    pub edge: AttrMap,
}

pub struct Parser<'a> {
    /// Read a token at a time, as bison calls `yylex`, so the scanner's warnings come when
    /// C's do: never past the token where a syntax error stops the graph.
    lexer: Lexer<'a>,
    /// The tokens read so far; the last is [`TokenKind::Eof`] once the input has ended.
    tokens: Vec<Token>,
    pos: usize,
    graph: Graph,
    scope_stack: Vec<SubgraphId>,
    globals: GlobalAttrs,
    /// Line of the root graph's closing `}`.
    rbrace_line: usize,
    /// Keys of the last attribute list in source order: cgraph declares them in this
    /// order, which fixes their `sym->id`.
    attr_order: Vec<String>,
    /// cgraph's id counter for anonymous objects (`ctr` in `idmap`).
    anon_ctr: u64,
    /// Parent of each subgraph (`None`: the root graph).
    sub_parent: Vec<Option<usize>>,
    /// Whether grammar.y's `startgraph` has run (the header was read).
    started: bool,
}

/// Which dictionary of a graph an attribute belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DictKind {
    Graph,
    Node,
    Edge,
}

impl<'a> Parser<'a> {
    fn new(lexer: Lexer<'a>, globals: &GlobalAttrs) -> Self {
        Self {
            lexer,
            tokens: Vec::new(),
            pos: 0,
            graph: Graph::new("", GraphKind::Undirected, false),
            scope_stack: Vec::new(),
            globals: globals.clone(),
            rbrace_line: 0,
            attr_order: Vec::new(),
            anon_ctr: 1,
            sub_parent: Vec::new(),
            started: false,
        }
    }

    /// Parses the first graph in `input`.
    pub fn parse_str(input: &'a str) -> Result<Graph> {
        let mut parser = Self::new(Lexer::new(input), &GlobalAttrs::default());
        parser.read_graph()?;
        Ok(parser.graph)
    }

    /// Parses every graph in `input`, applying `globals` before each graph body.
    pub fn parse_all(input: &'a str, globals: &GlobalAttrs) -> Result<Vec<Graph>> {
        match Self::parse_all_partial(input, globals) {
            (graphs, None) => Ok(graphs),
            (_, Some(e)) => Err(e),
        }
    }

    /// Like [`Parser::parse_all`], but keeps the graphs read before a syntax error: C's
    /// `agread` returns them one at a time, so `dot` lays out and renders those first.
    ///
    /// cgraph's scanner reads its input a line at a time (`fgets`) and each `agread`
    /// starts a new scanner, so whatever follows a graph's closing `}` on the same line
    /// is dropped, and the next graph is read from the next line, its line numbers
    /// counted from there.
    pub fn parse_all_partial(
        input: &'a str,
        globals: &GlobalAttrs,
    ) -> (Vec<Graph>, Option<GraphvizError>) {
        let mut graphs = Vec::new();
        // Built once: every `agread` goes on reading the same stream.
        let scan = ScanInput::new(input);
        let mut from = 0;
        loop {
            let mut parser = Parser::new(Lexer::from_scan_input(&scan, from), globals);
            if parser.peek().kind == TokenKind::Eof {
                return (graphs, None);
            }
            if let Err(e) = parser.read_graph() {
                return (graphs, Some(e));
            }
            graphs.push(parser.graph);
            match scan.line_end(from, parser.rbrace_line) {
                Some(end) => from = end,
                None => return (graphs, None),
            }
        }
    }

    fn peek(&mut self) -> &Token {
        self.peek_n(0)
    }

    /// The token `offset` places ahead, scanning up to it (the end of input stays the last).
    fn peek_n(&mut self, offset: usize) -> &Token {
        let want = self.pos + offset;
        while self.tokens.len() <= want
            && self.tokens.last().is_none_or(|t| t.kind != TokenKind::Eof)
        {
            self.tokens.push(self.lexer.scan());
        }
        let i = want.min(self.tokens.len() - 1);
        &self.tokens[i]
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if tok.kind != TokenKind::Eof {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &TokenKind) -> Result<Token> {
        if &self.peek().kind == expected {
            Ok(self.advance())
        } else {
            Err(syntax_error(self.peek()))
        }
    }

    fn is_atom_token(kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::Ident(_) | TokenKind::Quoted(_) | TokenKind::Html(_)
        )
    }

    /// `atom : T_atom | qatom` (grammar.y). The keywords `node`, `edge`,
    /// `graph`, `digraph`, `subgraph` and `strict` are their own tokens in
    /// scan.l (matched case-insensitively), not `T_atom`, so an unquoted
    /// keyword is a syntax error wherever an atom is expected: attribute
    /// names and values, node names, ports and compass points (issue #29).
    fn parse_atom(&mut self) -> Result<String> {
        let tok = self.peek().clone();
        let mut is_html = false;
        let mut val = match tok.kind {
            TokenKind::Ident(s) | TokenKind::Quoted(s) => {
                self.advance();
                s
            }
            TokenKind::Html(s) => {
                self.advance();
                is_html = true;
                s
            }
            _ => return Err(syntax_error(&tok)),
        };

        // Support string concatenation with `+` (`qatom '+' T_qatom` in grammar.y)
        while self.peek().kind == TokenKind::Plus {
            self.advance();
            let next_tok = self.peek().clone();
            match next_tok.kind {
                TokenKind::Ident(s) | TokenKind::Quoted(s) | TokenKind::Html(s) => {
                    self.advance();
                    is_html = false;
                    val.push_str(&s);
                }
                _ => return Err(syntax_error(&next_tok)),
            }
        }

        if is_html {
            self.graph.html_strings.insert(val.clone());
        }
        Ok(val)
    }

    /// Checks how many tokens an atom (including `+` concatenations) spans starting at `start_offset`,
    /// so we can look ahead for `=` without mutating parser state.
    fn atom_span_len(&mut self, start_offset: usize) -> Option<usize> {
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

    /// `graph : hdr body | error | /* empty */`. After a syntax error, bison's recovery
    /// reduces `graph : error`, whose `graph_error` ends the scan (`aglexeof`) only when
    /// `startgraph` opened a graph; before that, bison discards tokens until the end of the
    /// input, so the scanner reads them all, and warns as it does.
    fn read_graph(&mut self) -> Result<()> {
        self.started = false;
        let result = self.parse_graph();
        // (unless the input has already ended: the error token was the end, or `@`)
        if result.is_err()
            && !self.started
            && self.tokens.last().is_none_or(|t| t.kind != TokenKind::Eof)
        {
            while self.lexer.scan().kind != TokenKind::Eof {}
        }
        result
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
            _ => return Err(syntax_error(&tok)),
        };

        let name = if Self::is_atom_token(&self.peek().kind) {
            self.parse_atom()?
        } else {
            String::new()
        };

        self.started = true;
        self.anon_ctr = if name.is_empty() { 3 } else { 1 };
        self.graph = Graph::new(name, kind, strict);
        self.graph.graph_attrs.extend(self.globals.graph.clone());
        self.graph.node_defaults.extend(self.globals.node.clone());
        self.graph.edge_defaults.extend(self.globals.edge.clone());
        self.sub_parent.clear();
        self.graph.dicts = vec![graphviz_pure_core::AttrDicts::default()];
        // gvContext declares the node label on cgraph's ProtoGraph, which every root copies.
        self.graph.dicts[0].node.insert(
            "label".to_string(),
            graphviz_pure_core::AttrSym {
                defval: "\\N".to_string(),
                print: false,
                id: 0,
            },
        );
        // Command-line defaults come from cgraph's ProtoGraph dictionaries.
        for (kind, map) in [
            (DictKind::Graph, self.globals.graph.clone()),
            (DictKind::Node, self.globals.node.clone()),
            (DictKind::Edge, self.globals.edge.clone()),
        ] {
            for (k, v) in map {
                let d = self.dict_mut(0, kind);
                let id = d.get(&k).map_or(d.len(), |s| s.id);
                d.insert(
                    k,
                    graphviz_pure_core::AttrSym {
                        defval: v,
                        print: false,
                        id,
                    },
                );
            }
        }

        self.expect(&TokenKind::LBrace)?;
        self.parse_stmt_list()?;
        self.rbrace_line = self.expect(&TokenKind::RBrace)?.line;
        // A graph attribute first declared in a subgraph is declared on the root with
        // default "" (agattr), so agget(root, name) is "" rather than NULL.
        let declared: Vec<(String, String)> = self.graph.dicts[0]
            .graph
            .iter()
            .map(|(k, v)| (k.clone(), v.defval.clone()))
            .collect();
        for (k, v) in declared {
            self.graph.graph_attrs.entry(k).or_insert(v);
        }
        // `endgraph` ends the scan here (`aglexeof`), so nothing after the closing brace is
        // read; stray tokens on the next lines are the next read's (`parse_all_partial`).
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
        enum Stmt {
            Attr,
            Compound,
            Atom,
            Other,
        }
        let stmt = match &self.peek().kind {
            TokenKind::Graph | TokenKind::Node | TokenKind::Edge => Stmt::Attr,
            TokenKind::Subgraph | TokenKind::LBrace => Stmt::Compound,
            k if Self::is_atom_token(k) => Stmt::Atom,
            _ => Stmt::Other,
        };
        match stmt {
            Stmt::Attr => self.parse_attr_stmt(),
            Stmt::Compound => self.parse_compound(),
            Stmt::Atom => {
                if let Some(span) = self.atom_span_len(0)
                    && self.peek_n(span).kind == TokenKind::Equals
                {
                    let key = self.parse_atom()?;
                    self.expect(&TokenKind::Equals)?;
                    let val = self.parse_atom()?;
                    if !self.fixed_at_root(DictKind::Graph, &key) {
                        self.attr_stmt(DictKind::Graph, &key, &val);
                        self.current_graph_attrs_mut().insert(key, val);
                    } else {
                        if let Some(sym) = self.dict_mut(0, DictKind::Graph).get_mut(&key) {
                            sym.print = true;
                        }
                    }
                    return Ok(());
                }
                self.parse_compound()
            }
            Stmt::Other => Err(syntax_error(self.peek())),
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
            return Err(syntax_error(self.peek()));
        }

        let attrs = self.parse_opt_attr_list()?;
        let kind = match attr_tok.kind {
            TokenKind::Graph => DictKind::Graph,
            TokenKind::Node => DictKind::Node,
            _ => DictKind::Edge,
        };
        // `-G`/`-N`/`-E` symbols are `fixed`: root attribute statements keep them.
        let mut kept = AttrMap::new();
        for (k, v) in attrs {
            if self.fixed_at_root(kind, &k) {
                if let Some(sym) = self.dict_mut(0, kind).get_mut(&k) {
                    sym.print = true;
                }
            } else {
                kept.insert(k, v);
            }
        }
        let attrs = kept;
        for k in std::mem::take(&mut self.attr_order) {
            if let Some(v) = attrs.get(&k) {
                self.attr_stmt(kind, &k, v);
            }
        }
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
        self.attr_order.clear();
        while self.peek().kind == TokenKind::LBracket {
            self.advance(); // consume `[`
            // attrdefs: `atom '=' atom` followed by at most one `,` or `;`.
            while self.peek().kind != TokenKind::RBracket && self.peek().kind != TokenKind::Eof {
                let key = self.parse_atom()?;
                self.expect(&TokenKind::Equals)?;
                let val = self.parse_atom()?;
                if !attrs.contains_key(&key) {
                    self.attr_order.push(key.clone());
                }
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
                // scan.l returns the other graph type's edge operator as the token '-',
                // which no rule accepts.
                match (&op_tok.kind, self.graph.kind) {
                    (TokenKind::Arrow, GraphKind::Undirected)
                    | (TokenKind::DashDash, GraphKind::Directed) => {
                        return Err(syntax_error(&op_tok));
                    }
                    _ => {}
                }
                let next_item = self.parse_simple()?;
                chain.push(next_item.into_endpoints());
            }

            let edge_attrs = self.parse_opt_attr_list()?;
            for k in std::mem::take(&mut self.attr_order) {
                if k != "key" {
                    self.bind_attr(DictKind::Edge, &k);
                }
            }
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
            if matches!(first, SimpleItem::Nodes(_)) {
                for k in std::mem::take(&mut self.attr_order) {
                    self.bind_attr(DictKind::Node, &k);
                }
            }
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
            // `agfstnode(subg)..agnxtnode`: every node of the subgraph
            // (nested subgraphs included) in node sequence order.
            let mut ids: Vec<NodeId> = Vec::new();
            let mut stack = vec![subg_id];
            while let Some(s) = stack.pop() {
                let sg = &self.graph.subgraphs[s.0 as usize];
                ids.extend(sg.nodes.iter().copied());
                stack.extend(sg.subgraphs.iter().copied());
            }
            ids.sort_by_key(|n| n.0);
            ids.dedup();
            let endpoints = ids.into_iter().map(|nid| (nid, Port::default())).collect();
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
            let p1 = self.parse_atom()?;
            if self.peek().kind == TokenKind::Colon {
                self.advance();
                let p2 = self.parse_atom()?;
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
                line: tok.c_line,
                col: tok.col,
                message: format!("subgraphs nested more than {MAX_SUBGRAPH_DEPTH} deep"),
                file: None,
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
            return Err(syntax_error(self.peek()));
        }

        Ok(subg_id)
    }

    fn open_subgraph(&mut self, name: String) -> SubgraphId {
        let current_parent = self.scope_stack.last().copied();
        if !name.is_empty()
            && let Some(existing_id) = self.graph.subgraphs.iter().find_map(|s| {
                if s.name != name {
                    return None;
                }
                let has_same_parent = match current_parent {
                    Some(pid) => self.graph.subgraphs[pid.0 as usize]
                        .subgraphs
                        .contains(&s.id),
                    None => !self
                        .graph
                        .subgraphs
                        .iter()
                        .any(|p| p.subgraphs.contains(&s.id)),
                };
                has_same_parent.then_some(s.id)
            })
        {
            return existing_id;
        }

        let id = SubgraphId(self.graph.subgraphs.len() as u32);
        if name.is_empty() {
            self.graph.anon_subgraph_ids.insert(id.0, self.anon_ctr);
            self.anon_ctr += 2;
        }
        let is_cluster = name.to_ascii_lowercase().starts_with("cluster");
        let graph_attrs = self.current_graph_attrs().clone();
        let node_defaults = self.current_node_defaults().clone();
        let edge_defaults = self.current_edge_defaults().clone();

        self.sub_parent
            .push(self.scope_stack.last().map(|p| p.0 as usize));
        self.graph
            .dicts
            .push(graphviz_pure_core::AttrDicts::default());
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

    /// Dictionary index of the graph in scope (0: root).
    /// `sym->fixed` in `attrstmt`: a command-line default (`global_def`) that a
    /// root-level attribute statement does not change (subgraphs still may).
    fn fixed_at_root(&self, kind: DictKind, name: &str) -> bool {
        self.scope_dict() == 0
            && match kind {
                DictKind::Graph => self.globals.graph.contains_key(name),
                DictKind::Node => self.globals.node.contains_key(name),
                DictKind::Edge => self.globals.edge.contains_key(name),
            }
    }

    fn scope_dict(&self) -> usize {
        self.scope_stack.last().map_or(0, |s| s.0 as usize + 1)
    }

    fn dict_mut(&mut self, gi: usize, kind: DictKind) -> &mut AttrMap2 {
        let d = &mut self.graph.dicts[gi];
        match kind {
            DictKind::Graph => &mut d.graph,
            DictKind::Node => &mut d.node,
            DictKind::Edge => &mut d.edge,
        }
    }

    fn dict(&self, gi: usize, kind: DictKind) -> &AttrMap2 {
        let d = &self.graph.dicts[gi];
        match kind {
            DictKind::Graph => &d.graph,
            DictKind::Node => &d.node,
            DictKind::Edge => &d.edge,
        }
    }

    /// Dictionary lookup through the view path up to the root (`agdictsym`).
    fn lookup_view(&self, gi: usize, kind: DictKind, name: &str) -> bool {
        let mut cur = Some(gi);
        while let Some(g) = cur {
            if self.dict(g, kind).contains_key(name) {
                return true;
            }
            cur = if g == 0 {
                None
            } else {
                Some(self.sub_parent[g - 1].map_or(0, |p| p + 1))
            };
        }
        false
    }

    /// `bindattrs`: an attribute unknown in scope is declared on the root with default "".
    fn bind_attr(&mut self, kind: DictKind, name: &str) {
        let gi = self.scope_dict();
        if !self.lookup_view(gi, kind, name) {
            let d = self.dict_mut(0, kind);
            let id = d.len();
            d.insert(
                name.to_string(),
                graphviz_pure_core::AttrSym {
                    id,
                    ..Default::default()
                },
            );
        }
    }

    /// `attrstmt` + `setattr`: a default set in the current scope.
    fn attr_stmt(&mut self, kind: DictKind, name: &str, value: &str) {
        self.bind_attr(kind, name);
        let gi = self.scope_dict();
        if self.dict(gi, kind).contains_key(name) {
            if kind == DictKind::Graph {
                // unviewsubgraphsattr: direct children without a local value keep theirs.
                let children: Vec<usize> = if gi == 0 {
                    (0..self.sub_parent.len())
                        .filter(|&i| self.sub_parent[i].is_none())
                        .collect()
                } else {
                    self.graph.subgraphs[gi - 1]
                        .subgraphs
                        .iter()
                        .map(|c| c.0 as usize)
                        .collect()
                };
                for c in children {
                    if !self.graph.dicts[c + 1].graph.contains_key(name) {
                        let v = self.graph.subgraphs[c]
                            .graph_attrs
                            .get(name)
                            .cloned()
                            .unwrap_or_default();
                        self.graph.dicts[c + 1].graph.insert(
                            name.to_string(),
                            graphviz_pure_core::AttrSym {
                                defval: v,
                                ..Default::default()
                            },
                        );
                    }
                }
            }
            self.dict_mut(gi, kind).get_mut(name).unwrap().defval = value.to_string();
        } else {
            self.dict_mut(gi, kind).insert(
                name.to_string(),
                graphviz_pure_core::AttrSym {
                    defval: value.to_string(),
                    ..Default::default()
                },
            );
        }
        if gi == 0 {
            self.dict_mut(0, kind).get_mut(name).unwrap().print = true;
        }
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

        // agedge on a strict graph returns the existing edge, to which applyattrs adds
        // only this statement's attributes (the defaults applied when it was made).
        let undirected = self.graph.kind == graphviz_pure_core::GraphKind::Undirected;
        let exists = self.graph.strict
            && self.graph.edges.iter().any(|e| {
                (e.tail == tail && e.head == head)
                    || (undirected && e.tail == head && e.head == tail)
            });
        if exists {
            merged_attrs = edge_attrs.clone();
        }
        let before = self.graph.edges.len();
        let edge_id = self.graph.add_edge(tail, head, merged_attrs.clone());
        let is_new = self.graph.edges.len() > before;
        if is_new {
            self.anon_ctr += 2;
        }
        if let Some(edge) = self.graph.edges.get(edge_id.0 as usize) {
            // grammar.y `newedge`: an existing undirected edge found the other way round
            // takes the endpoint ports swapped.
            if edge.tail != edge.head && edge.head == tail {
                std::mem::swap(&mut tail_port, &mut head_port);
            }
            // `mkport` declares the `tailport`/`headport` edge symbol (default "") the
            // first time an `n:port` endpoint is seen, tail then head, after `bindattrs`
            // bound this statement's list. Symbol ids count it at that point, which the
            // nested flat-edge layout's stale `E_arrowsz` lookup depends on.
            for (key, endpoint) in [("tailport", &tail_port), ("headport", &head_port)] {
                if *endpoint != Port::default() {
                    self.bind_attr(DictKind::Edge, key);
                }
            }
        }
        if let Some(edge) = self.graph.edges.get_mut(edge_id.0 as usize) {
            // `newedge` sets the endpoint ports (`mkport`) after `agedge` applied the edge
            // defaults and before `applyattrs` applies the statement's attribute list, so
            // an explicit `tailport`/`headport` in the list wins over `n:port`, which wins
            // over a default.
            for (key, endpoint) in [("tailport", &tail_port), ("headport", &head_port)] {
                if *endpoint != Port::default() && !edge_attrs.contains_key(key) {
                    merged_attrs.insert(key.to_string(), port_to_str(endpoint));
                }
            }
            edge.attrs.extend(merged_attrs);
            edge.tail_port = edge
                .attrs
                .get("tailport")
                .map(|s| parse_port_str(s))
                .unwrap_or_default();
            edge.head_port = edge
                .attrs
                .get("headport")
                .map(|s| parse_port_str(s))
                .unwrap_or_default();
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

/// flex's `YY_BUF_SIZE`, which `aagerror` names as a possible cause.
const YY_BUF_SIZE: usize = 16384;

/// `aagerror` (`lib/cgraph/scan.l`) for bison's "syntax error" on lookahead token `tok`:
/// the line the scanner is on once `tok` is read, then `near '<yytext>'`, or, when yytext
/// is empty (end of input), the start condition the scanner was left in.
fn syntax_error(tok: &Token) -> GraphvizError {
    let mut message = format!("syntax error in line {}", tok.c_line);
    if !tok.text.is_empty() {
        message.push_str(&format!(" near '{}'", tok.text));
    } else {
        // `"%.80s"` of `Sbuf`: its first 80 bytes.
        let first_80 = |s: &str| {
            let bytes = graphviz_pure_core::charset::encode(s);
            graphviz_pure_core::charset::decode_slice(&bytes[..bytes.len().min(80)])
        };
        match &tok.state {
            ScanState::QString(sbuf) => {
                message.push_str(&format!(
                    " scanning a quoted string (missing endquote? longer than {YY_BUF_SIZE}?)"
                ));
                if !sbuf.is_empty() {
                    message.push_str(&format!("\nString starting:\"{}", first_80(sbuf)));
                }
            }
            ScanState::HString(sbuf) => {
                message.push_str(&format!(
                    " scanning a HTML string (missing '>'? bad nesting? longer than {YY_BUF_SIZE}?)"
                ));
                if !sbuf.is_empty() {
                    message.push_str(&format!("\nString starting:<{}", first_80(sbuf)));
                }
            }
            ScanState::Comment => message.push_str(&format!(
                " scanning a /*...*/ comment (missing '*/? longer than {YY_BUF_SIZE}?)"
            )),
            ScanState::Initial => {}
        }
    }
    GraphvizError::SyntaxError {
        line: tok.c_line,
        col: tok.col,
        message,
        file: tok.file.as_deref().map(str::to_string),
    }
}

/// grammar.y `concatPort`: `name` or `name:compass`.
fn port_to_str(p: &Port) -> String {
    match (&p.name, &p.compass) {
        (Some(n), Some(c)) => format!("{n}:{c}"),
        (Some(n), None) => n.clone(),
        (None, Some(c)) => c.clone(),
        (None, None) => String::new(),
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
