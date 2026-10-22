//! The failure boundary of the ortho router.
//!
//! `lib/ortho` asserts many of its invariants (`LIST_GET` checks every index,
//! `chanSearch` asserts its lookups) and relies on the rest without checking
//! (raw `N_DAD` and `cells[]` pointers, `int` distances). Where one does not
//! hold, C aborts, dereferences NULL, reads out of bounds or loops. The port
//! returns [`Fail`] from every function that can meet such a state, and
//! [`super::ortho_edges_of`] (`orthoEdges`) handles each failure in one
//! place, the way C handles the failure it does detect at that stage:
//!
//! - while the maze is built, as C's `mkMaze` returning NULL: `orthoEdges`
//!   returns -1;
//! - while edges are routed, as C's `goto orthofinish` after `shortPath` or
//!   `assignTracks` fails: no edge is routed and `orthoEdges` returns 0.
//!
//! Inputs C routes never fail, so their output does not change.

use std::panic::Location;

/// Why the router stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fail {
    /// C's own failure exits in `mkMaze`: a trapezoidation fails ("trapezoid
    /// segment construction failed") or `partition` leaves no free cell.
    NoMaze,
    /// `PQ_insert` finds the heap full; C prints "Heap overflow" and
    /// `shortPath` fails.
    HeapOverflow,
    /// `seg_cmp` meets two segments in different channels; C prints
    /// "incomparable segments !! -- Aborting" and `assignTracks` fails.
    Incomparable,
    /// `shortPath` never reached the edge's start node, so `N_DAD(sn)` is
    /// NULL and C's `convertSPtoRoute` dereferences it (#186).
    NoPath,
    /// A route segment lies in no channel: C's `chanSearch` asserts (#170).
    NoChannel,
    /// Any other invariant C asserts or trusts: the port's source line.
    Broken(&'static Location<'static>),
}

pub type OResult<T> = Result<T, Fail>;

impl Fail {
    /// [`Fail::Broken`] at the caller.
    #[track_caller]
    pub fn here() -> Fail {
        Fail::Broken(Location::caller())
    }
}

/// Fails unless `cond` holds: the port's form of a C `assert`.
#[track_caller]
pub fn ensure(cond: bool) -> OResult<()> {
    if cond { Ok(()) } else { Err(Fail::here()) }
}

/// Checked indexing: an index outside the data is a broken invariant.
pub trait At<T> {
    fn at(&self, i: usize) -> OResult<&T>;
    fn at_mut(&mut self, i: usize) -> OResult<&mut T>;
}

impl<T> At<T> for [T] {
    #[track_caller]
    fn at(&self, i: usize) -> OResult<&T> {
        match self.get(i) {
            Some(x) => Ok(x),
            None => Err(Fail::here()),
        }
    }

    #[track_caller]
    fn at_mut(&mut self, i: usize) -> OResult<&mut T> {
        match self.get_mut(i) {
            Some(x) => Ok(x),
            None => Err(Fail::here()),
        }
    }
}

/// A pointer C dereferences without a NULL check.
pub trait Need<T> {
    fn need(self) -> OResult<T>;
}

impl<T> Need<T> for Option<T> {
    #[track_caller]
    fn need(self) -> OResult<T> {
        match self {
            Some(x) => Ok(x),
            None => Err(Fail::here()),
        }
    }
}

/// An `int` index stored in the trapezoidation tables, as a `usize`.
#[track_caller]
pub fn idx(i: i32) -> OResult<usize> {
    usize::try_from(i).map_err(|_| Fail::here())
}
