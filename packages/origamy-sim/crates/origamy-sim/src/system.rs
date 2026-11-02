//! The Newton system: a sparse symmetric matrix in 3 by 3 blocks over pairs of vertices, filled
//! element by element in a fixed order, and solved by a sparse Cholesky factorization.

use faer::dyn_stack::{MemBuffer, MemStack};
use std::sync::Arc;

use faer::linalg::cholesky::llt::factor::{LltParams, LltRegularization};
use faer::sparse::linalg::cholesky::{
    CholeskySymbolicParams, LltRef, SymbolicCholesky, SymmetricOrdering,
    factorize_symbolic_cholesky,
};
use faer::sparse::{Argsort, Pair, SparseColMat, SymbolicSparseColMat};
use faer::{Conj, Mat, Par, Side, Spec};
use nalgebra::SMatrix;

/// Why the Newton system has no solution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "the Newton system is singular: some motion costs no energy, so hold a vertex or give the time step a finite length"
)]
pub struct SingularError;

/// The sparsity of the Newton system and its symbolic factorization, fixed for a scene.
#[derive(Clone, Debug)]
pub(crate) struct Pattern {
    /// Every block's vertex pair, the greater vertex first, in increasing order.
    blocks: Vec<(u32, u32)>,
    symbolic: SymbolicSparseColMat<usize>,
    argsort: Argsort<usize>,
    cholesky: Arc<SymbolicCholesky<usize>>,
    dimension: usize,
}

impl Pattern {
    /// The pattern of a system over `vertices` vertices whose elements join these pairs.
    pub fn new(vertices: usize, pairs: impl IntoIterator<Item = (u32, u32)>) -> Self {
        let mut blocks: Vec<_> = pairs
            .into_iter()
            .map(|(a, b)| (a.max(b), a.min(b)))
            .collect();
        let count = u32::try_from(vertices).expect("a mesh counts its vertices in 32 bits");
        blocks.extend((0..count).map(|v| (v, v)));
        blocks.sort_unstable();
        blocks.dedup();
        let indices: Vec<_> = blocks
            .iter()
            .flat_map(|&(a, b)| {
                (0..9).map(move |k| Pair {
                    row: 3 * a as usize + k / 3,
                    col: 3 * b as usize + k % 3,
                })
            })
            .collect();
        let dimension = 3 * vertices;
        let (symbolic, argsort) =
            SymbolicSparseColMat::try_new_from_indices(dimension, dimension, &indices)
                .expect("the pattern's indices lie inside the system");
        let cholesky = Arc::new(
            factorize_symbolic_cholesky(
                symbolic.as_ref(),
                Side::Lower,
                SymmetricOrdering::Amd,
                CholeskySymbolicParams::default(),
            )
            .expect("the symbolic factorization fits in memory"),
        );
        Self {
            blocks,
            symbolic,
            argsort,
            cholesky,
            dimension,
        }
    }

    /// Whether the pattern holds the block of a pair of vertices.
    pub fn contains(&self, a: u32, b: u32) -> bool {
        self.blocks.binary_search(&(a.max(b), a.min(b))).is_ok()
    }

    /// Where the block of rows `a` and columns `b` lies in the values, and whether it lies
    /// transposed there.
    fn block(&self, a: u32, b: u32) -> (usize, bool) {
        let key = (a.max(b), a.min(b));
        let index = self
            .blocks
            .binary_search(&key)
            .expect("every element's pairs belong to the pattern");
        (9 * index, a < b)
    }
}

/// A gradient and a Hessian being filled, over the free vertices of a scene.
pub(crate) struct Assembly<'a> {
    pattern: &'a Pattern,
    held: &'a [bool],
    pub gradient: Vec<f64>,
    values: Vec<f64>,
}

impl<'a> Assembly<'a> {
    /// An empty assembly; held vertices take no part, and their rows are the identity.
    pub fn new(pattern: &'a Pattern, held: &'a [bool]) -> Self {
        let mut values = vec![0.0; 9 * pattern.blocks.len()];
        for (v, _) in held.iter().enumerate().filter(|&(_, &h)| h) {
            let vertex = u32::try_from(v).expect("a mesh counts its vertices in 32 bits");
            let (offset, _) = pattern.block(vertex, vertex);
            for k in [0, 4, 8] {
                values[offset + k] = 1.0;
            }
        }
        Self {
            pattern,
            held,
            gradient: vec![0.0; pattern.dimension],
            values,
        }
    }

    /// Adds one element's gradient and Hessian over its stencil, with `D` = 3 times the stencil's
    /// length; `None` marks an empty slot.
    pub fn add<const D: usize>(
        &mut self,
        stencil: &[Option<u32>],
        gradient: &SMatrix<f64, D, 1>,
        hessian: &SMatrix<f64, D, D>,
    ) {
        for (i, a) in stencil.iter().enumerate() {
            let Some(a) = *a else { continue };
            if self.held[a as usize] {
                continue;
            }
            for c in 0..3 {
                self.gradient[3 * a as usize + c] += gradient[3 * i + c];
            }
            for (j, b) in stencil.iter().enumerate() {
                let Some(b) = *b else { continue };
                if self.held[b as usize] || b > a {
                    continue;
                }
                let (offset, transposed) = self.pattern.block(a, b);
                for r in 0..3 {
                    for c in 0..3 {
                        let (row, column) = if transposed { (c, r) } else { (r, c) };
                        self.values[offset + 3 * row + column] += hessian[(3 * i + r, 3 * j + c)];
                    }
                }
            }
        }
    }

    /// Adds a multiple of the identity to a vertex's diagonal block, and a vector to its gradient.
    pub fn add_vertex(&mut self, vertex: u32, gradient: [f64; 3], diagonal: f64) {
        if self.held[vertex as usize] {
            return;
        }
        for (c, component) in gradient.iter().enumerate() {
            self.gradient[3 * vertex as usize + c] += component;
        }
        let (offset, _) = self.pattern.block(vertex, vertex);
        for c in 0..3 {
            self.values[offset + 4 * c] += diagonal;
        }
    }

    /// Adds to a vertex's gradient and Hessian along z alone, as a force from the table does.
    pub fn add_vertex_z(&mut self, vertex: u32, slope: f64, curvature: f64) {
        if self.held[vertex as usize] {
            return;
        }
        self.gradient[3 * vertex as usize + 2] += slope;
        let (offset, _) = self.pattern.block(vertex, vertex);
        self.values[offset + 8] += curvature;
    }

    /// The Cholesky factorization of the Hessian.
    ///
    /// # Errors
    ///
    /// Returns [`SingularError`] when the Hessian is not positive definite.
    pub fn factor(&self) -> Result<Factor<'a>, SingularError> {
        let pattern = self.pattern;
        let matrix = SparseColMat::new_from_argsort(
            pattern.symbolic.clone(),
            &pattern.argsort,
            &self.values,
        )
        .expect("the values fill the pattern");
        let mut factor = vec![0.0; pattern.cholesky.len_val()];
        let par = Par::Seq;
        let mut memory = MemBuffer::new(
            pattern
                .cholesky
                .factorize_numeric_llt_scratch::<f64>(par, Spec::<LltParams, f64>::default()),
        );
        pattern
            .cholesky
            .factorize_numeric_llt(
                &mut factor,
                matrix.as_ref(),
                Side::Lower,
                LltRegularization::default(),
                par,
                MemStack::new(&mut memory),
                Spec::default(),
            )
            .map_err(|_| SingularError)?;
        Ok(Factor {
            pattern,
            values: factor,
        })
    }
}

/// A Cholesky factorization of the Newton system's matrix, for solving it again and again.
pub(crate) struct Factor<'a> {
    pattern: &'a Pattern,
    values: Vec<f64>,
}

impl Factor<'_> {
    /// The solution y of H y = `right`.
    pub fn solve(&self, right: &[f64]) -> Vec<f64> {
        let pattern = self.pattern;
        let par = Par::Seq;
        let mut memory = MemBuffer::new(pattern.cholesky.solve_in_place_scratch::<f64>(1, par));
        let mut solution = Mat::from_fn(pattern.dimension, 1, |i, _| right[i]);
        LltRef::new(&pattern.cholesky, &self.values).solve_in_place_with_conj(
            Conj::No,
            solution.as_mut(),
            par,
            MemStack::new(&mut memory),
        );
        (0..pattern.dimension).map(|i| solution[(i, 0)]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_system_solves_coupled_vertices() {
        // Two vertices joined by springs, the second held: only the first moves.
        let pattern = Pattern::new(2, [(0, 1)]);
        let held = [false, true];
        let mut assembly = Assembly::new(&pattern, &held);
        let mut hessian = SMatrix::<f64, 6, 6>::identity() * 2.0;
        hessian[(0, 3)] = -1.0;
        hessian[(3, 0)] = -1.0;
        let gradient = SMatrix::<f64, 6, 1>::from_column_slice(&[4.0, 0.0, -2.0, 9.0, 9.0, 9.0]);
        assembly.add(&[Some(0), Some(1)], &gradient, &hessian);
        let negative: Vec<f64> = assembly.gradient.iter().map(|g| -g).collect();
        let update = assembly.factor().unwrap().solve(&negative);
        let expected = [-2.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        assert!(
            update
                .iter()
                .zip(expected)
                .all(|(u, e)| (u - e).abs() < 1e-14)
        );
    }

    #[test]
    fn a_free_floating_system_is_singular() {
        let pattern = Pattern::new(1, []);
        let held = [false];
        let assembly = Assembly::new(&pattern, &held);
        assert!(matches!(assembly.factor(), Err(SingularError)));
    }
}
