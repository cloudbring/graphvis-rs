//! Port of `lib/ortho/rawgraph.c`: small directed graphs on segment indices
//! with a DFS topological sort.

const UNSCANNED: i32 = 0;
const SCANNING: i32 = 1;
const SCANNED: i32 = 2;

/// `vertex`.
#[derive(Debug, Clone, Default)]
pub struct Vertex {
    pub color: i32,
    pub topsort_order: i32,
    pub adj_list: Vec<usize>,
}

/// `rawgraph`.
#[derive(Debug, Clone, Default)]
pub struct RawGraph {
    pub vertices: Vec<Vertex>,
}

impl RawGraph {
    /// `make_graph(n)`.
    pub fn new(n: usize) -> Self {
        RawGraph {
            vertices: vec![Vertex::default(); n],
        }
    }

    /// `insert_edge(g, v1, v2)`.
    pub fn insert_edge(&mut self, v1: usize, v2: usize) {
        if !self.edge_exists(v1, v2) {
            self.vertices[v1].adj_list.push(v2);
        }
    }

    /// `remove_redge(g, v1, v2)`.
    pub fn remove_redge(&mut self, v1: usize, v2: usize) {
        if let Some(i) = self.vertices[v1].adj_list.iter().position(|&x| x == v2) {
            self.vertices[v1].adj_list.remove(i);
        }
        if let Some(i) = self.vertices[v2].adj_list.iter().position(|&x| x == v1) {
            self.vertices[v2].adj_list.remove(i);
        }
    }

    /// `edge_exists(g, v1, v2)`.
    pub fn edge_exists(&self, v1: usize, v2: usize) -> bool {
        self.vertices[v1].adj_list.contains(&v2)
    }

    fn dfs_visit(&mut self, v: usize, mut time: i32, sp: &mut Vec<usize>) -> i32 {
        self.vertices[v].color = SCANNING;
        let adj = self.vertices[v].adj_list.clone();
        time += 1;
        for id in adj {
            if self.vertices[id].color == UNSCANNED {
                time = self.dfs_visit(id, time, sp);
            }
        }
        self.vertices[v].color = SCANNED;
        sp.push(v);
        time + 1
    }

    /// `top_sort(g)`.
    pub fn top_sort(&mut self) {
        let n = self.vertices.len();
        if n == 0 {
            return;
        }
        if n == 1 {
            self.vertices[0].topsort_order = 0;
            return;
        }
        let mut time = 0;
        let mut sp = Vec::with_capacity(n);
        for i in 0..n {
            if self.vertices[i].color == UNSCANNED {
                time = self.dfs_visit(i, time, &mut sp);
            }
        }
        let mut count = 0;
        while let Some(v) = sp.pop() {
            self.vertices[v].topsort_order = count;
            count += 1;
        }
    }
}
