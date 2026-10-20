//! Single-source shortest paths: `lib/neatogen/bfs.c` (unweighted BFS) and
//! `lib/neatogen/dijkstra.c` (`ngdijkstra` on integer weights, `dijkstra_f`
//! on `float` weights, `dijkstra_sgd`), with C's binary heap so ties are
//! broken identically.

use super::graphdata::VtxData;

/// `DistType`.
pub type DistType = i32;

/// `bfs(vertex, graph, n, dist)`.
pub fn bfs(vertex: usize, graph: &[VtxData], n: usize, dist: &mut [DistType]) {
    let mut closest_dist: DistType = i32::MAX;
    for d in dist.iter_mut().take(n) {
        *d = -1;
    }
    dist[vertex] = 0;
    let mut q = std::collections::VecDeque::new();
    q.push_back(vertex);
    let weighted = graph[0].ewgts.is_some();
    while let Some(cv) = q.pop_front() {
        closest_dist = dist[cv];
        for i in 1..graph[cv].nedges() {
            let nb = graph[cv].edges[i] as usize;
            if f64::from(dist[nb]) < -0.5 {
                let bump = if weighted {
                    graph[cv].ewgts.as_ref().unwrap()[i] as DistType
                } else {
                    1
                };
                dist[nb] = closest_dist + bump;
                q.push_back(nb);
            }
        }
    }
    for d in dist.iter_mut().take(n) {
        if f64::from(*d) < -0.5 {
            *d = closest_dist + 10;
        }
    }
}

struct Heap {
    data: Vec<usize>,
    heap_size: usize,
    index: Vec<usize>,
}

fn left(i: usize) -> usize {
    2 * i
}
fn right(i: usize) -> usize {
    2 * i + 1
}
fn parent(i: usize) -> usize {
    i / 2
}

trait Key: Copy + PartialOrd {}
impl Key for i32 {}
impl Key for f32 {}

impl Heap {
    fn heapify<K: Key>(&mut self, mut i: usize, dist: &[K]) {
        loop {
            let l = left(i);
            let r = right(i);
            let mut largest = if l < self.heap_size && dist[self.data[l]] < dist[self.data[i]] {
                l
            } else {
                i
            };
            if r < self.heap_size && dist[self.data[r]] < dist[self.data[largest]] {
                largest = r;
            }
            if largest == i {
                break;
            }
            self.data.swap(largest, i);
            self.index[self.data[largest]] = largest;
            self.index[self.data[i]] = i;
            i = largest;
        }
    }

    fn new<K: Key>(start: usize, dist: &[K], n: usize) -> Self {
        let mut h = Heap {
            data: vec![0; n.saturating_sub(1)],
            heap_size: n.saturating_sub(1),
            index: vec![0; n],
        };
        let mut count = 0;
        for i in 0..n {
            if i != start {
                h.data[count] = i;
                h.index[i] = count;
                count += 1;
            }
        }
        let mut j = (n as isize - 1) / 2;
        while j >= 0 {
            h.heapify(j as usize, dist);
            j -= 1;
        }
        h
    }

    fn extract_max<K: Key>(&mut self, dist: &[K]) -> Option<usize> {
        if self.heap_size == 0 {
            return None;
        }
        let max = self.data[0];
        self.data[0] = self.data[self.heap_size - 1];
        self.index[self.data[0]] = 0;
        self.heap_size -= 1;
        self.heapify(0, dist);
        Some(max)
    }

    fn increase_key<K: Key>(&mut self, v: usize, new_dist: K, dist: &mut [K]) {
        if dist[v] <= new_dist {
            return;
        }
        let mut i = self.index[v];
        dist[v] = new_dist;
        while i > 0 && dist[self.data[parent(i)]] > new_dist {
            self.data[i] = self.data[parent(i)];
            self.index[self.data[i]] = i;
            i = parent(i);
        }
        self.data[i] = v;
        self.index[v] = i;
    }
}

const MAX_DIST: DistType = i32::MAX;

/// `ngdijkstra(vertex, graph, n, dist)`: integer distances.
pub fn ngdijkstra(vertex: usize, graph: &[VtxData], n: usize, dist: &mut [DistType]) {
    let mut prev_closest = MAX_DIST;
    for d in dist.iter_mut().take(n) {
        *d = MAX_DIST;
    }
    dist[vertex] = 0;
    let w = graph[vertex].ewgts.as_ref().unwrap();
    for i in 1..graph[vertex].nedges() {
        dist[graph[vertex].edges[i] as usize] = w[i] as DistType;
    }
    let mut h = Heap::new(vertex, dist, n);
    while let Some(cv) = h.extract_max(dist) {
        let cd = dist[cv];
        if cd == MAX_DIST {
            break;
        }
        let w = graph[cv].ewgts.as_ref().unwrap();
        for i in 1..graph[cv].nedges() {
            let nb = graph[cv].edges[i] as usize;
            h.increase_key(nb, cd.wrapping_add(w[i] as DistType), dist);
        }
        prev_closest = cd;
    }
    for d in dist.iter_mut().take(n) {
        if *d == MAX_DIST {
            *d = if i32::MAX - prev_closest < 10 {
                i32::MAX
            } else {
                prev_closest + 10
            };
        }
    }
}

/// `dijkstra_f(vertex, graph, n, dist)`: `float` distances; unreachable
/// vertices keep `FLT_MAX`.
pub fn dijkstra_f(vertex: usize, graph: &[VtxData], n: usize, dist: &mut [f32]) {
    for d in dist.iter_mut().take(n) {
        *d = f32::MAX;
    }
    dist[vertex] = 0.0;
    let w = graph[vertex].ewgts.as_ref().unwrap();
    for i in 1..graph[vertex].nedges() {
        dist[graph[vertex].edges[i] as usize] = w[i];
    }
    let mut h = Heap::new(vertex, dist, n);
    while let Some(cv) = h.extract_max(dist) {
        let cd = dist[cv];
        if cd == f32::MAX {
            break;
        }
        let w = graph[cv].ewgts.as_ref().unwrap();
        for i in 1..graph[cv].nedges() {
            let nb = graph[cv].edges[i] as usize;
            h.increase_key(nb, cd + w[i], dist);
        }
    }
}

/// A term of the SGD stress function (`term_sgd`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TermSgd {
    pub i: i32,
    pub j: i32,
    pub d: f32,
    pub w: f32,
}

/// `graph_sgd`: CSR adjacency with pinned flags.
pub struct GraphSgd {
    pub n: usize,
    pub sources: Vec<usize>,
    pub pinneds: Vec<bool>,
    pub targets: Vec<usize>,
    pub weights: Vec<f32>,
}

/// `dijkstra_sgd(graph, source, terms)`: appends terms, returns how many.
pub fn dijkstra_sgd(graph: &GraphSgd, source: usize, terms: &mut Vec<TermSgd>) -> usize {
    let mut dists = vec![f32::MAX; graph.n];
    dists[source] = 0.0;
    for i in graph.sources[source]..graph.sources[source + 1] {
        dists[graph.targets[i]] = graph.weights[i];
    }
    let mut h = Heap::new(source, &dists, graph.n);
    let mut offset = 0;
    while let Some(closest) = h.extract_max(&dists) {
        let d = dists[closest];
        if d == f32::MAX {
            break;
        }
        if graph.pinneds[closest] || closest < source {
            terms.push(TermSgd {
                i: source as i32,
                j: closest as i32,
                d,
                w: 1.0 / (d * d),
            });
            offset += 1;
        }
        for i in graph.sources[closest]..graph.sources[closest + 1] {
            let target = graph.targets[i];
            let weight = graph.weights[i];
            h.increase_key(target, d + weight, &mut dists);
        }
    }
    offset
}
