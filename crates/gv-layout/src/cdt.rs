//! Port of the ordered-set method of `lib/cdt` (`dttree.c` with `Dtoset`,
//! `dtflatten.c`, `dtrestore.c`): a top-down splay tree.
//!
//! Graphviz sometimes iterates a flattened dictionary (`dtflatten` +
//! `dtlink`) while searching the same dictionary; the search restores and
//! splays the tree, after which `dtlink` follows the tree's right pointers.
//! Reproducing which elements such loops visit needs the exact tree, hence
//! this port rather than a sorted vector.

use std::cmp::Ordering;

#[derive(Debug, Clone)]
struct Node<T> {
    obj: T,
    left: Option<usize>,
    right: Option<usize>,
}

/// `Dt_t` opened with `Dtoset`. Objects never leave the arena; indices are
/// the `Dtlink_t*` of C.
#[derive(Debug, Clone)]
pub struct Cdt<T> {
    nodes: Vec<Node<T>>,
    here: Option<usize>,
    flatten: bool,
}

impl<T> Default for Cdt<T> {
    fn default() -> Self {
        Cdt {
            nodes: Vec::new(),
            here: None,
            flatten: false,
        }
    }
}

/// A position in the search: a node or the local `link` header.
#[derive(Clone, Copy, PartialEq, Eq)]
enum P {
    Link,
    N(usize),
}

struct Link {
    left: Option<usize>,
    right: Option<usize>,
}

impl<T> Cdt<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, i: usize) -> &T {
        &self.nodes[i].obj
    }

    pub fn get_mut(&mut self, i: usize) -> &mut T {
        &mut self.nodes[i].obj
    }

    /// `dtlink(dt, e)`: the right pointer of `e`.
    pub fn link(&self, e: usize) -> Option<usize> {
        self.nodes[e].right
    }

    fn set_left(&mut self, link: &mut Link, p: P, v: Option<usize>) {
        match p {
            P::Link => link.left = v,
            P::N(i) => self.nodes[i].left = v,
        }
    }

    fn set_right(&mut self, link: &mut Link, p: P, v: Option<usize>) {
        match p {
            P::Link => link.right = v,
            P::N(i) => self.nodes[i].right = v,
        }
    }

    fn left_of(&self, link: &Link, p: P) -> Option<usize> {
        match p {
            P::Link => link.left,
            P::N(i) => self.nodes[i].left,
        }
    }

    /// `rrotate(x, y)`.
    fn rrotate(&mut self, x: usize, y: usize) {
        self.nodes[x].left = self.nodes[y].right;
        self.nodes[y].right = Some(x);
    }

    /// `lrotate(x, y)`.
    fn lrotate(&mut self, x: usize, y: usize) {
        self.nodes[x].right = self.nodes[y].left;
        self.nodes[y].left = Some(x);
    }

    /// `dtflatten(dt)`: turn the tree into a list linked by `right`; returns its head.
    pub fn flatten(&mut self) -> Option<usize> {
        if self.flatten {
            return self.here;
        }
        let mut list = None;
        if let Some(mut r) = self.here {
            while let Some(t) = self.nodes[r].left {
                self.rrotate(r, t);
                r = t;
            }
            list = Some(r);
            let mut last = r;
            let mut cur = self.nodes[r].right;
            while let Some(mut rr) = cur {
                if let Some(mut t) = self.nodes[rr].left {
                    loop {
                        self.rrotate(rr, t);
                        rr = t;
                        match self.nodes[rr].left {
                            Some(t2) => t = t2,
                            None => break,
                        }
                    }
                    self.nodes[last].right = Some(rr);
                }
                last = rr;
                cur = self.nodes[rr].right;
            }
        }
        self.here = list;
        self.flatten = true;
        list
    }

    /// `UNFLATTEN(dt)` / `dtrestore(dt, NULL)`.
    fn unflatten(&mut self) {
        if self.flatten {
            self.flatten = false;
        }
    }

    /// The search part of `dttree` for `DT_MATCH`/`DT_SEARCH`/`DT_INSERT`:
    /// returns the node found, leaving `link` and the `l`/`r` cursors.
    fn search(
        &mut self,
        cmp: &dyn Fn(&T) -> Ordering,
        link: &mut Link,
        l: &mut P,
        r: &mut P,
    ) -> Option<usize> {
        let mut root = self.here?;
        loop {
            let c = cmp(&self.nodes[root].obj);
            if c == Ordering::Equal {
                return Some(root);
            } else if c == Ordering::Less {
                if let Some(t) = self.nodes[root].left {
                    let c2 = cmp(&self.nodes[t].obj);
                    if c2 == Ordering::Less {
                        self.rrotate(root, t);
                        self.set_left(link, *r, Some(t));
                        *r = P::N(t);
                        root = self.nodes[t].left?;
                    } else if c2 == Ordering::Equal {
                        self.set_left(link, *r, Some(root));
                        *r = P::N(root);
                        return Some(t);
                    } else {
                        self.set_right(link, *l, Some(t));
                        *l = P::N(t);
                        self.set_left(link, *r, Some(root));
                        *r = P::N(root);
                        root = self.nodes[t].right?;
                    }
                } else {
                    self.set_left(link, *r, Some(root));
                    *r = P::N(root);
                    return None;
                }
            } else if let Some(t) = self.nodes[root].right {
                let c2 = cmp(&self.nodes[t].obj);
                if c2 == Ordering::Greater {
                    self.lrotate(root, t);
                    self.set_right(link, *l, Some(t));
                    *l = P::N(t);
                    root = self.nodes[t].right?;
                } else if c2 == Ordering::Equal {
                    self.set_right(link, *l, Some(root));
                    *l = P::N(root);
                    return Some(t);
                } else {
                    self.set_left(link, *r, Some(t));
                    *r = P::N(t);
                    self.set_right(link, *l, Some(root));
                    *l = P::N(root);
                    root = self.nodes[t].left?;
                }
            } else {
                self.set_right(link, *l, Some(root));
                *l = P::N(root);
                return None;
            }
        }
    }

    /// `has_root`: make `root` the tree root.
    fn has_root(&mut self, root: usize, link: &Link) -> usize {
        self.nodes[root].left = link.right;
        self.nodes[root].right = link.left;
        self.here = Some(root);
        root
    }

    /// `no_root`: rebuild the tree from the left and right parts.
    fn no_root(&mut self, link: &mut Link, mut r: P) {
        while let Some(t) = self.left_of(link, r) {
            r = P::N(t);
        }
        let lr = link.right;
        self.set_left(link, r, lr);
        self.here = link.left;
    }

    /// `dtmatch(dt, key)` / `dtsearch(dt, obj)`: `cmp(obj)` compares the key
    /// with an element (`_DTCMP(key, k)`).
    pub fn find(&mut self, cmp: &dyn Fn(&T) -> Ordering) -> Option<usize> {
        self.unflatten();
        self.here?;
        let mut link = Link {
            left: None,
            right: None,
        };
        let mut l = P::Link;
        let mut r = P::Link;
        match self.search(cmp, &mut link, &mut l, &mut r) {
            Some(root) => {
                let (rl, rr) = (self.nodes[root].left, self.nodes[root].right);
                self.set_right(&mut link, l, rl);
                self.set_left(&mut link, r, rr);
                Some(self.has_root(root, &link))
            }
            None => {
                self.set_left(&mut link, r, None);
                self.set_right(&mut link, l, None);
                self.no_root(&mut link, r);
                None
            }
        }
    }

    /// `dtinsert(dt, obj)` for an ordered set: returns the existing element
    /// equal to `obj`, or the newly inserted one, and whether it is new.
    pub fn insert(&mut self, obj: T, cmp: &dyn Fn(&T) -> Ordering) -> (usize, bool) {
        self.unflatten();
        let mut link = Link {
            left: None,
            right: None,
        };
        let mut l = P::Link;
        let mut r = P::Link;
        if self.here.is_some() {
            if let Some(root) = self.search(cmp, &mut link, &mut l, &mut r) {
                let (rl, rr) = (self.nodes[root].left, self.nodes[root].right);
                self.set_right(&mut link, l, rl);
                self.set_left(&mut link, r, rr);
                return (self.has_root(root, &link), false);
            }
            self.set_left(&mut link, r, None);
            self.set_right(&mut link, l, None);
        }
        self.nodes.push(Node {
            obj,
            left: None,
            right: None,
        });
        let root = self.nodes.len() - 1;
        (self.has_root(root, &link), true)
    }

    /// `dtnext(dt, obj)` for an element `e` of the dictionary.
    pub fn next(&mut self, e: usize, cmp: &dyn Fn(&T) -> Ordering) -> Option<usize> {
        self.unflatten();
        let mut root = self.here?;
        let mut link = Link {
            left: None,
            right: None,
        };
        let mut l = P::Link;
        let mut r = P::Link;
        if root != e {
            match self.search(cmp, &mut link, &mut l, &mut r) {
                Some(x) => root = x,
                None => {
                    self.set_left(&mut link, r, None);
                    self.set_right(&mut link, l, None);
                    // dt_next with no root
                    return self.dt_next(&mut link, r);
                }
            }
        }
        let (rl, rr) = (self.nodes[root].left, self.nodes[root].right);
        self.set_right(&mut link, l, rl);
        self.set_left(&mut link, r, rr);
        self.nodes[root].left = link.right;
        self.nodes[root].right = None;
        link.right = Some(root);
        self.dt_next(&mut link, r)
    }

    fn dt_next(&mut self, link: &mut Link, r: P) -> Option<usize> {
        if let Some(mut root) = link.left {
            while let Some(t) = self.nodes[root].left {
                self.rrotate(root, t);
                root = t;
            }
            link.left = self.nodes[root].right;
            Some(self.has_root(root, link))
        } else {
            self.no_root(link, r);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordered_iteration() {
        let mut d: Cdt<i32> = Cdt::new();
        for v in [5, 1, 9, 3, 7, 3] {
            d.insert(v, &|o: &i32| v.cmp(o));
        }
        let mut out = Vec::new();
        let mut e = d.flatten();
        while let Some(i) = e {
            out.push(*d.get(i));
            e = d.link(i);
        }
        assert_eq!(out, vec![1, 3, 5, 7, 9]);
        let k = d.find(&|o: &i32| 7.cmp(o)).unwrap();
        assert_eq!(*d.get(k), 7);
        let n = d.next(k, &|o: &i32| 7.cmp(o)).unwrap();
        assert_eq!(*d.get(n), 9);
    }
}
