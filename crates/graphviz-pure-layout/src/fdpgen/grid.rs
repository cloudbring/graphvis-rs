//! Uniform grid of cells for the repulsive forces, ported from
//! `lib/fdpgen/grid.c`.
//!
//! C keeps cells in an ordered cdt set keyed by `(i, j)` and prepends nodes to
//! each cell's list; a [`BTreeMap`] with the same key order and lists read
//! back to front reproduce both orders.

use std::collections::BTreeMap;

/// `Grid`: cell `(i, j)` → nodes in insertion order (C walks them reversed).
#[derive(Debug, Default)]
pub struct Grid {
    cells: BTreeMap<(i32, i32), Vec<usize>>,
}

impl Grid {
    /// `mkGrid(cellHint)`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `clearGrid(g)`.
    pub fn clear(&mut self) {
        self.cells.clear();
    }

    /// `addGrid(g, i, j, n)`.
    pub fn add(&mut self, i: i32, j: i32, n: usize) {
        self.cells.entry((i, j)).or_default().push(n);
    }

    /// `findGrid(g, i, j)`: the cell's nodes in C list order (last added first).
    pub fn find(&self, i: i32, j: i32) -> Option<impl Iterator<Item = usize> + '_> {
        self.cells.get(&(i, j)).map(|v| v.iter().rev().copied())
    }

    /// Cells in `walkGrid` order, each with its nodes in C list order.
    pub fn walk_order(&self) -> Vec<((i32, i32), Vec<usize>)> {
        self.cells
            .iter()
            .map(|(&k, v)| (k, v.iter().rev().copied().collect()))
            .collect()
    }
}
