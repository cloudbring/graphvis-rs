use crate::geom::{BoundingBox, Point, Spline};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EdgeId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SubgraphId(pub u32);

pub type AttrMap = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GraphKind {
    Undirected,
    Directed,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Port {
    pub name: Option<String>,
    pub compass: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeData {
    pub id: NodeId,
    pub name: String,
    pub attrs: AttrMap,
    /// Layout center in points (72 pt/inch).
    pub pos: Point,
    /// Width in inches (Graphviz convention).
    pub width: f64,
    /// Height in inches (Graphviz convention).
    pub height: f64,
    pub rank: i32,
    pub order: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EdgeData {
    pub id: EdgeId,
    pub tail: NodeId,
    pub head: NodeId,
    pub tail_port: Port,
    pub head_port: Port,
    pub attrs: AttrMap,
    pub spline: Spline,
    pub label_pos: Option<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubgraphData {
    pub id: SubgraphId,
    pub name: String,
    pub is_cluster: bool,
    pub graph_attrs: AttrMap,
    pub node_defaults: AttrMap,
    pub edge_defaults: AttrMap,
    pub nodes: Vec<NodeId>,
    pub edges: Vec<EdgeId>,
    pub subgraphs: Vec<SubgraphId>,
    pub bb: BoundingBox,
}

/// Arena-backed Graph representation (Fixes GV-BUG-008 and GV-BUG-010).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub name: String,
    pub strict: bool,
    pub kind: GraphKind,
    pub graph_attrs: AttrMap,
    pub node_defaults: AttrMap,
    pub edge_defaults: AttrMap,
    pub nodes: Vec<NodeData>,
    pub node_lookup: BTreeMap<String, NodeId>,
    pub edges: Vec<EdgeData>,
    pub subgraphs: Vec<SubgraphData>,
    pub bb: BoundingBox,
    /// Attribute values that were written as HTML strings (`<...>`) in the source, stored with
    /// their delimiters. A quoted `"<x>"` is a plain string and is not recorded here.
    #[serde(default)]
    pub html_strings: BTreeSet<String>,
    /// cgraph's internal ids of anonymous subgraphs (subgraph index -> id); the root graph,
    /// anonymous subgraphs and edges draw odd ids from one counter in creation order. C
    /// prints an anonymous subgraph's name as `%<id>`.
    #[serde(default)]
    pub anon_subgraph_ids: BTreeMap<u32, u64>,
    /// cgraph attribute dictionaries (`Agdatadict_t`) as built while parsing: index 0 is the
    /// root graph, index `i + 1` subgraph `i`. Writers that reproduce `agwrite` use them to
    /// tell local declarations from inherited values.
    #[serde(default)]
    pub dicts: Vec<AttrDicts>,
}

/// One declared attribute (`Agsym_t`): its default value and whether a root attribute
/// statement set it (`sym->print`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AttrSym {
    pub defval: String,
    pub print: bool,
}

/// The local graph, node and edge dictionaries of one graph or subgraph.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AttrDicts {
    pub graph: BTreeMap<String, AttrSym>,
    pub node: BTreeMap<String, AttrSym>,
    pub edge: BTreeMap<String, AttrSym>,
}

impl Graph {
    pub fn new(name: impl Into<String>, kind: GraphKind, strict: bool) -> Self {
        Self {
            name: name.into(),
            strict,
            kind,
            graph_attrs: AttrMap::new(),
            node_defaults: AttrMap::new(),
            edge_defaults: AttrMap::new(),
            nodes: Vec::new(),
            node_lookup: BTreeMap::new(),
            edges: Vec::new(),
            subgraphs: Vec::new(),
            bb: BoundingBox::default(),
            html_strings: BTreeSet::new(),
            anon_subgraph_ids: BTreeMap::new(),
            dicts: Vec::new(),
        }
    }

    pub fn ensure_node(&mut self, name: &str) -> NodeId {
        if let Some(&id) = self.node_lookup.get(name) {
            return id;
        }
        let id = NodeId(self.nodes.len() as u32);
        let mut attrs = self.node_defaults.clone();
        attrs
            .entry("label".to_string())
            .or_insert_with(|| "\\N".to_string());
        self.nodes.push(NodeData {
            id,
            name: name.to_string(),
            attrs,
            pos: Point::default(),
            width: 0.75,
            height: 0.5,
            rank: 0,
            order: id.0 as usize,
        });
        self.node_lookup.insert(name.to_string(), id);
        id
    }

    pub fn add_edge(&mut self, tail: NodeId, head: NodeId, attrs: AttrMap) -> EdgeId {
        if self.strict
            && let Some(existing) = self.edges.iter_mut().find(|e| {
                (e.tail == tail && e.head == head)
                    || (self.kind == GraphKind::Undirected && e.tail == head && e.head == tail)
            })
        {
            existing.attrs.extend(attrs);
            return existing.id;
        }
        let id = EdgeId(self.edges.len() as u32);
        let mut merged_attrs = self.edge_defaults.clone();
        merged_attrs.extend(attrs);
        self.edges.push(EdgeData {
            id,
            tail,
            head,
            tail_port: Port::default(),
            head_port: Port::default(),
            attrs: merged_attrs,
            spline: Spline::default(),
            label_pos: None,
        });
        id
    }
}
