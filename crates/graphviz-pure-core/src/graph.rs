use crate::geom::{BoundingBox, Point, Spline};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
    /// The name was an HTML string (`<b>`): `name` holds its inner text, and cgraph's
    /// `aghtmlstr` is set on it. Writers print it as `<b>`; everything else sees `b`.
    #[serde(default)]
    pub html_name: bool,
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
    /// The layout could not place the label (`ED_label(e)->set` is false):
    /// `lp` is still printed, but renderers do not draw it.
    #[serde(default)]
    pub label_unplaced: bool,
    /// The cgraph name of the edge: the `key` of the statement that made it (`agedge`'s
    /// `name`). Defaults never name an edge (`edge [key=…]`, `-Ekey=…`), although `-E`
    /// gives every edge a `key` attribute.
    #[serde(default)]
    pub key: Option<String>,
    /// The key is an HTML string (`<...>`): cgraph's string identity includes the flag, so
    /// `key=<k>` and `key="k"` name two edges, and `agwrite` prints it back as `<k>`.
    #[serde(default)]
    pub key_html: bool,
}

/// Prefix of the private key that marks an attribute value as an HTML string (cgraph's
/// `aghtmlstr` on the value): `attrs["\u{1}html:label"]` holds the value of `attrs["label"]`
/// when that was written `<...>`. The value itself is the inner text, which is what every
/// reader but the label makers and the writers sees. The mark is a key of the same map so
/// that it travels with the value when the map is cloned or merged, as cgraph's flag travels
/// with its string; it holds the value it marks so that writing another value (`agxset` of a
/// plain string, such as a computed `bb`) unmarks it.
pub const HTML_VALUE_PREFIX: &str = "\u{1}html:";

/// The key of the mark [`HTML_VALUE_PREFIX`] puts on `key`.
pub fn html_value_key(key: &str) -> String {
    format!("{HTML_VALUE_PREFIX}{key}")
}

/// `aghtmlstr(agget(obj, key))`: whether the value of `key` in `attrs` is an HTML string.
pub fn attr_is_html(attrs: &AttrMap, key: &str) -> bool {
    attrs
        .get(&html_value_key(key))
        .is_some_and(|marked| attrs.get(key) == Some(marked))
}

/// `agxset` / `agxset_html`: sets `key` to `value`, an HTML string's inner text when `html`.
pub fn set_attr(attrs: &mut AttrMap, key: &str, value: String, html: bool) {
    let mark = html_value_key(key);
    if html {
        attrs.insert(mark, value.clone());
    } else {
        attrs.remove(&mark);
    }
    attrs.insert(key.to_string(), value);
}

/// Sets every attribute of `src` on `attrs`, each with its HTML mark (`applyattrs`).
pub fn apply_attrs(attrs: &mut AttrMap, src: &AttrMap) {
    for (k, v) in src {
        if !k.starts_with(HTML_VALUE_PREFIX) {
            set_attr(attrs, k, v.clone(), attr_is_html(src, k));
        }
    }
}

/// The name test of `is_a_cluster` (lib/common/utils.c): `strncasecmp(name, "cluster", 7)`.
///
/// This compares bytes, as C does. Slicing the `str` at byte 7 instead panics when byte 7
/// falls inside a multi-byte character (issue #165).
pub fn has_cluster_prefix(name: &str) -> bool {
    name.as_bytes()
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case(b"cluster"))
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
    /// The name was an HTML string; `name` holds its inner text (see [`NodeData::html_name`]).
    #[serde(default)]
    pub html_name: bool,
    /// The subgraph is named by the empty string (`subgraph "" {…}`), which cgraph keeps
    /// as a name: only a missing name makes a subgraph anonymous (grammar.y `opensubg`).
    #[serde(default)]
    pub empty_name: bool,
}

impl SubgraphData {
    /// `agnameof` gives no name of the subgraph's own: it has none, or a local `%` name.
    pub fn is_anonymous(&self) -> bool {
        (self.name.is_empty() && !self.empty_name) || self.name.starts_with('%')
    }
}

/// Node attribute set by a layout on a node it replaced by a cluster node
/// (fdp's `processClusterEdges`/`undoClusterEdges`, `IS_CLUST_NODE`):
/// renderers neither draw it nor give it layout attributes.
pub const CLUST_NODE_KEY: &str = "\u{1}clustnode";

/// Subgraph attribute set by a layout that registers a cluster without making its label
/// (`GD_label` stays NULL, e.g. patchwork's `mkClusters`): renderers neither draw nor size it.
pub const NO_GRAPH_LABEL_KEY: &str = "\u{1}nolabel";

/// Root graph attribute set by a layout whose output keeps every edge in the subgraphs it
/// was declared in: only dot's `node_induce` adds the edges between a cluster's nodes to
/// the cluster, and not when doDot lays out pack components (it adds them to the
/// components' clones).
pub const NO_LOW_CLUSTERS_KEY: &str = "\u{1}nolowclust";

/// Root graph attribute set by a layout that binds edge records without calling
/// `common_init_edge` (patchwork's `patchwork_init_edge`): edges have no labels.
pub const NO_EDGE_INIT_KEY: &str = "\u{1}noedgeinit";

/// Node or edge attribute of a [`Graph::render_view`] object: its cgraph sequence number
/// (`AGSEQ`), which the renderers print in object ids. Without it the sequence follows the
/// object's index.
pub const SEQ_KEY: &str = "\u{1}seq";

/// Subgraph attribute set by a layout on a cluster it did not register in
/// `GD_clust` (e.g. clusters nested below neato's top-level ones): it keeps its
/// input `bb`, which C prints, but renderers do not draw it.
pub const UNREGISTERED_CLUSTER_KEY: &str = "\u{1}unregclust";

/// Subgraph attribute set by a layout on a cluster it registered in `GD_clust` but never
/// positioned (fdp stopping on a layout error): renderers draw it with its empty box.
pub const REGISTERED_CLUSTER_KEY: &str = "\u{1}regclust";

/// Root graph or cluster attribute set by a layout whose `GD_clust` list is not the
/// subgraph tree: the subgraph indices of the list, in order and with repeats,
/// separated by spaces. dot's pack branch (`copyClusterInfo`) builds the root's
/// list in component order and appends a cluster once per component that kept it;
/// `emit_clusters` draws the list as it is.
pub const CLUSTER_LIST_KEY: &str = "\u{1}clustlist";

/// Edge attribute set by a layout whose head or tail label it could not place
/// (`ED_head_label(e)->set` false: `h`, tail: `t`): `head_lp`/`tail_lp` are still
/// printed, but renderers do not draw the label.
pub const UNPLACED_PORT_LABELS_KEY: &str = "\u{1}unplacedportlbl";
/// Edge attributes holding the layout's final tail/head port names
/// (`ED_tail_port(e).name`, `ED_head_port(e).name`), which `\E` substitutes; empty when
/// the layout dropped the name (dot's `sameport`).
pub const TAIL_PORT_NAME_KEY: &str = "\u{1}tailportname";
pub const HEAD_PORT_NAME_KEY: &str = "\u{1}headportname";
/// Edge attribute set by dot on an edge whose arrowheads `arrow_flags` merged with an
/// opposing edge's (`ED_conc_opp_flag` under `concentrate`): the arrow flags of each of
/// its splines (`bz.sflag`, `bz.eflag`) as `"s,e"`, separated by spaces. Which opposing
/// edge cgraph finds depends on its dictionaries, so renderers take the flags from here.
pub const CONC_ARROW_FLAGS_KEY: &str = "\u{1}concarrows";
/// One spelling for [`IGNORED_EDGE_KEY`] and the [`IGNORED_EDGE_INPUT_KEYS`] built on it.
macro_rules! ignored_edge_key {
    ($attr:literal) => {
        concat!("\u{1}ignorededge", $attr)
    };
}
/// Edge attribute set by a layout on an edge it routed although its type is `IGNORED`
/// (dot's `orthoEdges` under `concentrate`, which keeps the first edge of each node pair
/// whatever its type): renderers draw its spline (`emit_edge_graphics` has no type check),
/// but `attach_attrs_and_arrows` gives it no `pos`, `lp`, `xlp`, `head_lp` or `tail_lp`
/// (`lib/common/output.c`). The layout keeps the edge's input label positions under
/// [`IGNORED_EDGE_INPUT_KEYS`], since it may overwrite them to draw the labels.
pub const IGNORED_EDGE_KEY: &str = ignored_edge_key!("");
/// `(attribute, key)`: where the layout keeps the input value of a label position attribute
/// of an [`IGNORED_EDGE_KEY`] edge, for the renderer to put back.
pub const IGNORED_EDGE_INPUT_KEYS: [(&str, &str); 3] = [
    ("head_lp", ignored_edge_key!("head_lp")),
    ("tail_lp", ignored_edge_key!("tail_lp")),
    ("xlp", ignored_edge_key!("xlp")),
];

/// Label position attributes a layout writes on nodes and edges to pass its result to the
/// renderers (`ED_head_label(e)->pos` and the like), which C keeps out of the cgraph until
/// `attach_attrs` runs.
pub const LAYOUT_LABEL_POS_KEYS: [&str; 3] = ["xlp", "head_lp", "tail_lp"];
/// Prefix of the node or edge attribute keeping the input value of a
/// [`LAYOUT_LABEL_POS_KEYS`] attribute the layout overwrote (an empty key after it: none),
/// for the writers that print the graph without the layout's attributes.
pub const LAYOUT_INPUT_PREFIX: &str = "\u{1}input:";
/// Prefix of the node or edge attribute marking a [`LAYOUT_LABEL_POS_KEYS`] attribute the
/// layout set where the input had none.
pub const LAYOUT_NO_INPUT_PREFIX: &str = "\u{1}noinput:";

/// Node attribute set by a layout whose node has `ND_lw != ND_rw` (dot keeps the self-loop
/// space in `ND_rw` when `dot_splines_` skips `resetRW`, or with `splines=curved` undoes
/// it): `"lw,rw"` in points. Renderers split
/// `width` with it instead of halving it (`Mcircle_hack` uses `ND_rw` alone).
pub const NODE_LW_RW_KEY: &str = "\u{1}lwrw";

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
    /// The graph as renderers that run before `undoClusterEdges` see it: fdp leaves its
    /// cluster nodes (`__0:cluster`) and cloned cluster edges in the graph it lays out, and
    /// only the output writers (dot, xdot, json, canon, plain) undo them. `-Tsvg` draws this
    /// graph when it is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_view: Option<Box<Graph>>,
    /// The root is named by the empty string (`digraph "" {…}`); an empty `name` is
    /// otherwise an anonymous root (`agopen(NULL)`), which cgraph calls `%1`.
    #[serde(default)]
    pub empty_name: bool,
}

/// One declared attribute (`Agsym_t`): its default value and whether a root attribute
/// statement set it (`sym->print`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AttrSym {
    pub defval: String,
    pub print: bool,
    /// `sym->id`: declaration order in the root dictionary (cgraph's ProtoGraph
    /// `label` first). Only meaningful in the root's dictionaries.
    #[serde(default)]
    pub id: usize,
    /// `aghtmlstr(sym->name)`: the attribute was first named by an HTML string (`<zzz>=1`).
    /// The name is its inner text, which is how cgraph looks attributes up.
    #[serde(default)]
    pub html: bool,
    /// `aghtmlstr(sym->defval)`: the default is an HTML string (its inner text).
    #[serde(default)]
    pub html_defval: bool,
    /// Declared on cgraph's ProtoGraph (the node `label`, `-G`/`-N`/`-E`), which every root
    /// graph copies with its ids: a graph made later, like dot's aux graph, numbers these
    /// symbols the same way.
    #[serde(default)]
    pub proto: bool,
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
            anon_subgraph_ids: BTreeMap::new(),
            dicts: Vec::new(),
            render_view: None,
            empty_name: false,
        }
    }

    /// The root has no name of its own (see [`Graph::empty_name`]).
    pub fn is_anonymous(&self) -> bool {
        (self.name.is_empty() && !self.empty_name) || self.name.starts_with('%')
    }

    pub fn ensure_node(&mut self, name: &str) -> NodeId {
        self.ensure_node_html(name, false)
    }

    /// The `node_lookup` key of a node named `name`. cgraph keeps an HTML string apart from
    /// the plain string with the same text (`refstr_eq` compares the flag), so `<b>` and `b`
    /// are two nodes. An HTML name is keyed with a NUL in front, which no C string holds.
    pub fn node_key(name: &str, html: bool) -> std::borrow::Cow<'_, str> {
        if html {
            std::borrow::Cow::Owned(format!("\0{name}"))
        } else {
            std::borrow::Cow::Borrowed(name)
        }
    }

    /// [`Graph::ensure_node`] for a node whose name may be an HTML string.
    pub fn ensure_node_html(&mut self, name: &str, html: bool) -> NodeId {
        let key = Self::node_key(name, html);
        if let Some(&id) = self.node_lookup.get(key.as_ref()) {
            return id;
        }
        let key = key.into_owned();
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
            html_name: html,
        });
        self.node_lookup.insert(key, id);
        id
    }

    /// `agdelete(root, n)` of every node `delete` marks: the nodes go, with their edges, from
    /// the graph and every subgraph. The objects left keep their cgraph sequence numbers
    /// ([`SEQ_KEY`]), which object ids in the output are made of.
    pub fn delete_nodes(&mut self, delete: &[bool]) {
        let mut node_map = vec![None; self.nodes.len()];
        let mut nodes = Vec::with_capacity(self.nodes.len());
        for (i, mut n) in std::mem::take(&mut self.nodes).into_iter().enumerate() {
            if delete.get(i).copied().unwrap_or(false) {
                continue;
            }
            n.attrs
                .entry(SEQ_KEY.to_string())
                .or_insert_with(|| (i + 1).to_string());
            let id = NodeId(nodes.len() as u32);
            node_map[i] = Some(id);
            n.id = id;
            nodes.push(n);
        }
        self.nodes = nodes;
        let mut edge_map = vec![None; self.edges.len()];
        let mut edges = Vec::with_capacity(self.edges.len());
        for (i, mut e) in std::mem::take(&mut self.edges).into_iter().enumerate() {
            let (Some(t), Some(h)) = (node_map[e.tail.0 as usize], node_map[e.head.0 as usize])
            else {
                continue;
            };
            e.attrs
                .entry(SEQ_KEY.to_string())
                .or_insert_with(|| (i + 1).to_string());
            let id = EdgeId(edges.len() as u32);
            edge_map[i] = Some(id);
            (e.id, e.tail, e.head) = (id, t, h);
            edges.push(e);
        }
        self.edges = edges;
        for sg in &mut self.subgraphs {
            sg.nodes = sg
                .nodes
                .iter()
                .filter_map(|n| node_map[n.0 as usize])
                .collect();
            sg.edges = sg
                .edges
                .iter()
                .filter_map(|e| edge_map[e.0 as usize])
                .collect();
        }
        self.node_lookup = self.nodes.iter().map(|n| (n.name.clone(), n.id)).collect();
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
        let mut merged_attrs = self.edge_defaults.clone();
        merged_attrs.extend(attrs);
        self.new_edge(tail, head, merged_attrs)
    }

    /// Makes an edge with exactly `attrs`, even where a strict graph already has one
    /// (`agedge` does, for a new key in a subgraph that lacks the edge).
    pub fn new_edge(&mut self, tail: NodeId, head: NodeId, attrs: AttrMap) -> EdgeId {
        let id = EdgeId(self.edges.len() as u32);
        self.edges.push(EdgeData {
            id,
            tail,
            head,
            tail_port: Port::default(),
            head_port: Port::default(),
            attrs,
            spline: Spline::default(),
            label_pos: None,
            label_unplaced: false,
            key: None,
            key_html: false,
        });
        id
    }
}

#[cfg(test)]
mod cluster_prefix_tests {
    use super::has_cluster_prefix;

    #[test]
    fn compares_the_first_seven_bytes_case_insensitively() {
        assert!(has_cluster_prefix("cluster"));
        assert!(has_cluster_prefix("Cluster_a"));
        assert!(has_cluster_prefix("CLUSTERé"));
        assert!(!has_cluster_prefix("cluste"));
        assert!(!has_cluster_prefix("aaaaaaé"));
        assert!(!has_cluster_prefix("clusé_x"));
        assert!(!has_cluster_prefix(""));
    }
}
