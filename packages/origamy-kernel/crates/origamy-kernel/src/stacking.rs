//! The layer solver: which face lies above which, wherever two faces overlap, as
//! `origamy.layers` defines it.
//!
//! Each pair of overlapping faces is a variable: positive puts the lesser face above.
//! Folds and flat creases give clauses, and transitivity forbids any three faces that
//! share area from stacking in a cycle. Three faces share area exactly when they meet
//! in a region of the footprints' overlay where the most footprints meet, so each such
//! region keeps the orders among its faces as rows of bits, closed under transitivity:
//! learning that one face lies above another adds every order it implies there at once.

#![allow(
    clippy::many_single_char_names,
    reason = "faces f, g and h, and literals x and y, follow the papers' names"
)]

use std::collections::BTreeSet;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::geometry::{Line, Point};
use crate::sheet::{Pair, Sheet};

/// One geometric reason that limits how some faces stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    /// `taco-taco`, `taco-tortilla`, `tortilla-tortilla` or `transitivity`.
    pub kind: &'static str,
    /// The faces involved.
    pub faces: Vec<u32>,
    /// The crease the constraint is about, by its index among the sheet's creases.
    pub crease: Option<u32>,
}

/// A constraint as the problem keeps it: its kind, up to four faces, and its crease.
#[derive(Clone, Copy, Debug)]
struct Origin {
    kind: &'static str,
    faces: [u32; 4],
    count: u8,
    crease: u32,
}

impl Origin {
    fn new(kind: &'static str, faces: &[u32], crease: u32) -> Origin {
        let mut all = [0; 4];
        all[..faces.len()].copy_from_slice(faces);
        Origin {
            kind,
            faces: all,
            count: u8::try_from(faces.len()).expect("at most four faces"),
            crease,
        }
    }

    fn constraint(self) -> Constraint {
        Constraint {
            kind: self.kind,
            faces: self.faces[..usize::from(self.count)].to_vec(),
            crease: Some(self.crease),
        }
    }
}

/// Why no stacking was found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackError {
    /// No stacking keeps paper from crossing paper: the constraints, and the required
    /// orders, as `(upper, lower)`, that conflict.
    NoStacking {
        /// The constraints behind the conflict, in order.
        constraints: Vec<Constraint>,
        /// The required orders behind it.
        fixed: Vec<Pair>,
    },
    /// The search ran out of dead ends to try before finding a stacking.
    Budget,
}

/// A crease as the solver sees it: its faces and where it lies once folded.
struct Crease {
    index: u32,
    /// A fold's two faces, or a flat crease's two faces, the positive side's first.
    faces: [u32; 2],
    ends: (Point, Point),
    /// Its line once folded, by index among the overlay's lines.
    line: u32,
    /// For a fold, the side of its line both faces lie on; 0 for a flat crease.
    side: i32,
}

/// The stacking constraints of a sheet: clauses, and transitivity found on demand.
pub struct Problem<'a> {
    sheet: &'a Sheet,
    /// Where each face's neighbors start in `neighbors`.
    rows: Vec<u32>,
    /// Each face's overlapping faces, sorted, each with the literal that puts the face
    /// above it.
    neighbors: Vec<(u32, i32)>,
    clause_starts: Vec<u32>,
    clause_literals: Vec<i32>,
    origins: Vec<u32>,
    constraints: Vec<Origin>,
    /// The faces of each region where the most footprints meet, sorted.
    regions: Vec<Vec<u32>>,
    /// The regions holding each face, sorted, each with the face's place among the
    /// region's faces.
    holding: Vec<Vec<(u32, u32)>>,
}

impl<'a> Problem<'a> {
    /// Every constraint of a sheet's geometry.
    ///
    /// # Panics
    ///
    /// When the constraints or their literals number 2³² or more.
    #[must_use]
    pub fn new(sheet: &'a Sheet) -> Problem<'a> {
        let mut rows = vec![0u32; sheet.folded.len() + 1];
        for &(f, g) in &sheet.overlapping {
            rows[f as usize + 1] += 1;
            rows[g as usize + 1] += 1;
        }
        for f in 0..sheet.folded.len() {
            rows[f + 1] += rows[f];
        }
        let mut fill: Vec<u32> = rows.clone();
        let mut neighbors = vec![(0, 0); 2 * sheet.overlapping.len()];
        for (v, &(f, g)) in sheet.overlapping.iter().enumerate() {
            let literal = signed(v + 1);
            neighbors[fill[f as usize] as usize] = (g, literal);
            fill[f as usize] += 1;
            neighbors[fill[g as usize] as usize] = (f, -literal);
            fill[g as usize] += 1;
        }
        // The pairs come sorted, so each face's neighbors below it come first and in
        // order, then those above it in order: a merge sorts them.
        for f in 0..sheet.folded.len() {
            neighbors[rows[f] as usize..rows[f + 1] as usize].sort_unstable_by_key(|&(g, _)| g);
        }
        let regions: Vec<Vec<u32>> = sheet
            .overlay
            .regions
            .iter()
            .map(|cover| {
                let mut faces: Vec<u32> = cover
                    .iter()
                    .flat_map(|&k| sheet.stacked[k as usize].iter().copied())
                    .collect();
                faces.sort_unstable();
                faces
            })
            .filter(|faces| faces.len() > 2)
            .collect();
        let mut holding = vec![vec![]; sheet.folded.len()];
        for (r, faces) in regions.iter().enumerate() {
            let r = u32::try_from(r).expect("regions fit in 32 bits");
            for (k, &f) in faces.iter().enumerate() {
                holding[f as usize].push((r, u32::try_from(k).expect("faces fit in 32 bits")));
            }
        }
        let mut problem = Problem {
            sheet,
            rows,
            neighbors,
            clause_starts: vec![0],
            clause_literals: vec![],
            origins: vec![],
            constraints: vec![],
            regions,
            holding,
        };
        let creases = problem.creases();
        problem.folds_and_flats(&creases);
        problem.crossings(&creases);
        problem
    }

    /// How many pairs of faces overlap: the variables.
    #[must_use]
    pub fn variables(&self) -> usize {
        self.sheet.overlapping.len()
    }

    /// The literal that says `upper` is above `lower`, if the two overlap.
    ///
    /// # Panics
    ///
    /// When the pairs number 2³¹ or more.
    #[must_use]
    pub fn literal(&self, upper: u32, lower: u32) -> Option<i32> {
        let row = self.neighbors_of(upper);
        let at = row.binary_search_by_key(&lower, |&(g, _)| g).ok()?;
        Some(row[at].1)
    }

    /// A face's overlapping faces, sorted, each with the literal that puts the face
    /// above it.
    fn neighbors_of(&self, face: u32) -> &[(u32, i32)] {
        &self.neighbors[self.rows[face as usize] as usize..self.rows[face as usize + 1] as usize]
    }

    fn pair(&self, literal: i32) -> Pair {
        let (f, g) = self.sheet.overlapping[literal.unsigned_abs() as usize - 1];
        if literal > 0 { (f, g) } else { (g, f) }
    }

    fn clause(&self, index: u32) -> &[i32] {
        &self.clause_literals[self.clause_starts[index as usize] as usize
            ..self.clause_starts[index as usize + 1] as usize]
    }

    fn add(&mut self, origin: Origin, clauses: &[&[i32]]) {
        let index = u32::try_from(self.constraints.len()).expect("constraints fit in 32 bits");
        self.constraints.push(origin);
        for clause in clauses {
            self.clause_literals.extend_from_slice(clause);
            self.clause_starts
                .push(u32::try_from(self.clause_literals.len()).expect("literals fit in 32 bits"));
            self.origins.push(index);
        }
    }

    fn equal(&mut self, origin: Origin, x: Option<i32>, y: Option<i32>) {
        if let (Some(x), Some(y)) = (x, y) {
            if x != y {
                self.add(origin, &[&[-x, y], &[x, -y]]);
            }
        }
    }

    fn creases(&self) -> Vec<Crease> {
        let sheet = self.sheet;
        sheet
            .creases
            .iter()
            .enumerate()
            .map(|(index, &([v, w], [f, g]))| {
                let ends = (sheet.image(f, v).clone(), sheet.image(f, w).clone());
                let line = sheet.overlay.crease_lines[index];
                let side = sheet.inside(f, v, w);
                let index = u32::try_from(index).expect("creases fit in 32 bits");
                if sheet.same_place(f, g) {
                    let faces = if side > 0 { [f, g] } else { [g, f] };
                    Crease {
                        index,
                        faces,
                        ends,
                        line,
                        side: 0,
                    }
                } else {
                    Crease {
                        index,
                        faces: [f, g],
                        ends,
                        line,
                        side,
                    }
                }
            })
            .collect()
    }

    fn folds_and_flats(&mut self, creases: &[Crease]) {
        let tower = &self.sheet.tower;
        let mut lines: FxHashMap<u32, usize> = FxHashMap::default();
        let mut groups: Vec<Vec<&Crease>> = vec![];
        for crease in creases {
            let next = groups.len();
            let k = *lines.entry(crease.line).or_insert(next);
            if k == groups.len() {
                groups.push(vec![]);
            }
            groups[k].push(crease);
        }
        for group in &groups {
            let line = &self.sheet.overlay.arrangement.lines[group[0].line as usize];
            for (i, j) in overlapping_stretches(group, line, tower) {
                let (first, second) = (group[i], group[j]);
                if first.side != 0 && second.side != 0 {
                    if first.side == second.side {
                        self.taco_taco(first, second);
                    }
                } else if first.side != 0 || second.side != 0 {
                    let (taco, flat) = if first.side != 0 {
                        (first, second)
                    } else {
                        (second, first)
                    };
                    let tortilla = if taco.side > 0 {
                        flat.faces[0]
                    } else {
                        flat.faces[1]
                    };
                    self.taco_tortilla(taco, tortilla);
                } else {
                    self.tortilla_tortilla(first, second);
                }
            }
        }
    }

    fn taco_taco(&mut self, first: &Crease, second: &Crease) {
        let ([a, b], [c, d]) = (first.faces, second.faces);
        if repeats(&[a, b, c, d]) {
            return;
        }
        let (Some(ca), Some(cb), Some(da), Some(db)) = (
            self.literal(c, a),
            self.literal(c, b),
            self.literal(d, a),
            self.literal(d, b),
        ) else {
            return;
        };
        let known = [ca, cb, da, db];
        // c lies between a and b when exactly one of a and b is below it; so does d.
        // Nesting or stacking puts both or neither between: the parity is even.
        let clauses: Vec<[i32; 4]> = (0..16u32)
            .filter(|bits| bits.count_ones() % 2 == 1)
            .map(|bits| {
                std::array::from_fn(|i| {
                    if bits & (8 >> i) != 0 {
                        -known[i]
                    } else {
                        known[i]
                    }
                })
            })
            .collect();
        let slices: Vec<&[i32]> = clauses.iter().map(<[i32; 4]>::as_slice).collect();
        self.add(
            Origin::new("taco-taco", &[a, b, c, d], first.index),
            &slices,
        );
    }

    fn taco_tortilla(&mut self, taco: &Crease, tortilla: u32) {
        let [a, b] = taco.faces;
        if tortilla == a || tortilla == b {
            return;
        }
        let (x, y) = (self.literal(tortilla, a), self.literal(tortilla, b));
        self.equal(
            Origin::new("taco-tortilla", &[a, b, tortilla], taco.index),
            x,
            y,
        );
    }

    fn tortilla_tortilla(&mut self, first: &Crease, second: &Crease) {
        let ([p1, n1], [p2, n2]) = (first.faces, second.faces);
        if repeats(&[p1, n1, p2, n2]) {
            return;
        }
        let (x, y) = (self.literal(p1, p2), self.literal(n1, n2));
        self.equal(
            Origin::new("tortilla-tortilla", &[p1, n1, p2, n2], first.index),
            x,
            y,
        );
    }

    fn crossings(&mut self, creases: &[Crease]) {
        let sheet = self.sheet;
        for crease in creases {
            let [a, b] = crease.faces;
            let kind = if crease.side != 0 {
                "taco-tortilla"
            } else {
                "tortilla-tortilla"
            };
            for &k in &sheet.overlay.crossed[crease.index as usize] {
                for &face in &sheet.stacked[k as usize] {
                    if face != a && face != b {
                        let (x, y) = (self.literal(face, a), self.literal(face, b));
                        self.equal(Origin::new(kind, &[a, b, face], crease.index), x, y);
                    }
                }
            }
        }
    }

    /// The regions holding both faces of a pair, each with the two faces' places among
    /// its faces.
    fn shared(&self, (f, g): Pair, out: &mut Vec<(u32, u32, u32)>) {
        out.clear();
        let (a, b) = (&self.holding[f as usize], &self.holding[g as usize]);
        let (mut i, mut j) = (0, 0);
        while i < a.len() && j < b.len() {
            match a[i].0.cmp(&b[j].0) {
                std::cmp::Ordering::Less => i += 1,
                std::cmp::Ordering::Greater => j += 1,
                std::cmp::Ordering::Equal => {
                    out.push((a[i].0, a[i].1, b[j].1));
                    i += 1;
                    j += 1;
                }
            }
        }
    }

    /// A cycle's three literals, putting f above g, g above h and h above f, with
    /// f < g < h: a stacking makes some of them hold, and not all.
    fn cycle(&self, faces: [u32; 3]) -> [i32; 3] {
        let [f, g, h] = faces;
        let variable = |x: u32, y: u32| self.literal(x, y).expect("faces of a cycle overlap");
        [variable(f, g), variable(g, h), -variable(f, h)]
    }
}

/// Why a variable holds its value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reason {
    /// A choice, or a required order.
    Given,
    /// A clause, by index.
    Clause(u32),
    /// A cycle of three faces, f < g < h.
    Cycle([u32; 3]),
}

/// Unit propagation and chronological backtracking over a problem's constraints.
struct Solver<'p, 'a> {
    problem: &'p Problem<'a>,
    values: Vec<i8>,
    reasons: Vec<Reason>,
    trail: Vec<i32>,
    containing_starts: Vec<u32>,
    containing: Vec<u32>,
    orders: Vec<Orders>,
    scratch: Vec<(u32, u32, u32)>,
    lowers: Vec<u64>,
    uppers: Vec<usize>,
}

/// The orders known among a region's faces, closed under transitivity, as bits: row
/// `x` of `below` holds the faces below face `x`, and of `above` those above it.
struct Orders {
    words: usize,
    above: Vec<u64>,
    below: Vec<u64>,
}

impl Orders {
    fn new(size: usize) -> Orders {
        let words = size.div_ceil(64);
        Orders {
            words,
            above: vec![0; size * words],
            below: vec![0; size * words],
        }
    }

    fn holds(&self, x: usize, y: usize) -> bool {
        self.below[x * self.words + y / 64] >> (y % 64) & 1 == 1
    }

    fn set(&mut self, x: usize, y: usize) {
        self.below[x * self.words + y / 64] |= 1 << (y % 64);
        self.above[y * self.words + x / 64] |= 1 << (x % 64);
    }

    fn clear(&mut self, x: usize, y: usize) {
        self.below[x * self.words + y / 64] &= !(1 << (y % 64));
        self.above[y * self.words + x / 64] &= !(1 << (x % 64));
    }
}

/// The set bits of some words, in increasing order.
fn bits(words: &[u64]) -> impl Iterator<Item = usize> + '_ {
    words.iter().enumerate().flat_map(|(w, &word)| {
        let mut rest = word;
        std::iter::from_fn(move || {
            if rest == 0 {
                return None;
            }
            let bit = rest.trailing_zeros() as usize;
            rest &= rest - 1;
            Some(w * 64 + bit)
        })
    })
}

/// A variable as a positive literal.
fn signed(variable: usize) -> i32 {
    i32::try_from(variable).expect("variables fit in 31 bits")
}

fn slot(literal: i32) -> usize {
    2 * literal.unsigned_abs() as usize + usize::from(literal < 0)
}

impl<'p, 'a> Solver<'p, 'a> {
    fn new(problem: &'p Problem<'a>) -> Self {
        let variables = problem.variables() + 1;
        let mut starts = vec![0u32; 2 * variables + 1];
        for &literal in &problem.clause_literals {
            starts[slot(literal) + 1] += 1;
        }
        for i in 0..2 * variables {
            starts[i + 1] += starts[i];
        }
        let mut fill = starts.clone();
        let mut containing = vec![0u32; problem.clause_literals.len()];
        for index in 0..u32::try_from(problem.origins.len()).expect("clauses fit in 32 bits") {
            for &literal in problem.clause(index) {
                let s = slot(literal);
                containing[fill[s] as usize] = index;
                fill[s] += 1;
            }
        }
        Solver {
            problem,
            values: vec![0; variables],
            reasons: vec![Reason::Given; variables],
            trail: vec![],
            containing_starts: starts,
            containing,
            orders: problem
                .regions
                .iter()
                .map(|faces| Orders::new(faces.len()))
                .collect(),
            scratch: vec![],
            lowers: vec![],
            uppers: vec![],
        }
    }

    fn value(&self, literal: i32) -> i8 {
        let value = self.values[literal.unsigned_abs() as usize];
        if literal > 0 { value } else { -value }
    }

    fn assign(&mut self, literal: i32, reason: Reason) {
        let variable = literal.unsigned_abs() as usize;
        self.values[variable] = if literal > 0 { 1 } else { -1 };
        self.reasons[variable] = reason;
        self.trail.push(literal);
    }

    /// Deduce from the trail after `start`; return a constraint that fails, if any.
    fn propagate(&mut self, start: usize) -> Option<Reason> {
        let mut head = start;
        let mut regions = std::mem::take(&mut self.scratch);
        let mut conflict = None;
        'trail: while head < self.trail.len() {
            let falsified = -self.trail[head];
            head += 1;
            let s = slot(falsified);
            for k in self.containing_starts[s]..self.containing_starts[s + 1] {
                let index = self.containing[k as usize];
                let mut open = None;
                let mut satisfied = false;
                for &literal in self.problem.clause(index) {
                    let value = self.value(literal);
                    if value > 0 || (value == 0 && open.is_some()) {
                        satisfied = true; // satisfied, or two literals still open
                        break;
                    }
                    if value == 0 {
                        open = Some(literal);
                    }
                }
                if !satisfied {
                    match open {
                        None => {
                            conflict = Some(Reason::Clause(index));
                            break 'trail;
                        }
                        Some(literal) => self.assign(literal, Reason::Clause(index)),
                    }
                }
            }
            let (upper, lower) = self.problem.pair(-falsified);
            self.problem.shared((upper, lower), &mut regions);
            for &(r, u, l) in &regions {
                if let Some(found) =
                    self.close(r as usize, (upper, u as usize), (lower, l as usize))
                {
                    conflict = Some(found);
                    break 'trail;
                }
            }
        }
        self.scratch = regions;
        conflict
    }

    /// Add to a region that `upper` lies above `lower`, with every order it implies
    /// there; return a cycle that closes, if any.
    fn close(
        &mut self,
        r: usize,
        (upper, u): (u32, usize),
        (lower, l): (u32, usize),
    ) -> Option<Reason> {
        let faces = &self.problem.regions[r];
        if self.orders[r].holds(u, l) {
            return None;
        }
        let words = self.orders[r].words;
        // Every face at or above `upper` goes above every face at or below `lower`.
        let mut lowers = std::mem::take(&mut self.lowers);
        let mut uppers = std::mem::take(&mut self.uppers);
        lowers.clear();
        lowers.extend_from_slice(&self.orders[r].below[l * words..(l + 1) * words]);
        lowers[l / 64] |= 1 << (l % 64);
        uppers.clear();
        uppers.push(u);
        uppers.extend(bits(&self.orders[r].above[u * words..(u + 1) * words]));
        let triple = |x: usize, y: usize| {
            let mut t = if x == u {
                [upper, lower, faces[y]]
            } else if y == l {
                [faces[x], upper, lower]
            } else {
                [faces[x], upper, faces[y]]
            };
            t.sort_unstable();
            t
        };
        let mut found = None;
        'uppers: for &x in &uppers {
            let row = x * words;
            for (w, &low) in lowers.iter().enumerate() {
                let orders = &self.orders[r];
                let mut new = low & !orders.below[row + w];
                while new != 0 {
                    let y = w * 64 + new.trailing_zeros() as usize;
                    new &= new - 1;
                    if y == x || orders.holds(y, x) {
                        found = Some(Reason::Cycle(triple(x, y)));
                        break 'uppers;
                    }
                }
            }
            // The faces below come in order, so the search for each resumes where the
            // last one ended among the neighbors of face x.
            let neighbors = self.problem.neighbors_of(faces[x]);
            let mut start = 0;
            for (w, &low) in lowers.iter().enumerate() {
                let mut new = low & !self.orders[r].below[row + w];
                while new != 0 {
                    let y = w * 64 + new.trailing_zeros() as usize;
                    new &= new - 1;
                    self.orders[r].set(x, y);
                    if x == u && y == l {
                        continue;
                    }
                    start += neighbors[start..].partition_point(|&(g, _)| g < faces[y]);
                    let literal = neighbors[start].1;
                    match self.value(literal) {
                        0 => self.assign(literal, Reason::Cycle(triple(x, y))),
                        -1 => {
                            found = Some(Reason::Cycle(triple(x, y)));
                            break 'uppers;
                        }
                        _ => {}
                    }
                }
            }
        }
        self.lowers = lowers;
        self.uppers = uppers;
        found
    }

    fn literals(&self, reason: Reason) -> Vec<i32> {
        match reason {
            Reason::Given => vec![],
            Reason::Clause(index) => self.problem.clause(index).to_vec(),
            Reason::Cycle(faces) => self.problem.cycle(faces).to_vec(),
        }
    }

    /// The constraints and required literals behind a conflict at the root.
    fn explain(&self, conflict: Reason) -> (Vec<Constraint>, Vec<Pair>) {
        let mut clauses = BTreeSet::new();
        let mut cycles = BTreeSet::new();
        let mut required = BTreeSet::new();
        let mut seen = FxHashSet::default();
        let mut pending = vec![conflict];
        while let Some(reason) = pending.pop() {
            match reason {
                Reason::Clause(index) => {
                    clauses.insert(index);
                }
                Reason::Cycle(faces) => {
                    cycles.insert(faces);
                }
                Reason::Given => {}
            }
            for literal in self.literals(reason) {
                let variable = literal.unsigned_abs() as usize;
                if !seen.insert(variable) {
                    continue;
                }
                match self.reasons[variable] {
                    Reason::Given => {
                        required.insert(signed(variable) * i32::from(self.values[variable]));
                    }
                    reason => pending.push(reason),
                }
            }
        }
        let problem = self.problem;
        let mut constraints: Vec<Constraint> = clauses
            .iter()
            .map(|&index| {
                problem.constraints[problem.origins[index as usize] as usize].constraint()
            })
            .collect();
        constraints.extend(cycles.iter().map(|faces| Constraint {
            kind: "transitivity",
            faces: faces.to_vec(),
            crease: None,
        }));
        (
            constraints,
            required
                .iter()
                .map(|&literal| problem.pair(literal))
                .collect(),
        )
    }

    fn undo(&mut self, mark: usize) {
        let mut regions = std::mem::take(&mut self.scratch);
        for k in mark..self.trail.len() {
            let literal = self.trail[k];
            let variable = literal.unsigned_abs() as usize;
            self.values[variable] = 0;
            self.reasons[variable] = Reason::Given;
            let pair = self.problem.sheet.overlapping[variable - 1];
            self.problem.shared(pair, &mut regions);
            for &(r, a, b) in &regions {
                let (a, b) = (a as usize, b as usize);
                self.orders[r as usize].clear(a, b);
                self.orders[r as usize].clear(b, a);
            }
        }
        self.trail.truncate(mark);
        self.scratch = regions;
    }

    /// Complete the assignment, choosing preferred literals first; `None` when the
    /// budget of dead ends runs out.
    fn search(&mut self, order: &[i32], mut budget: i64) -> Option<bool> {
        let mut preferred: FxHashMap<usize, i32> = FxHashMap::default();
        let mut queue = vec![];
        for &literal in order {
            let variable = literal.unsigned_abs() as usize;
            if preferred.insert(variable, literal).is_none() {
                queue.push(variable);
            }
        }
        queue.extend((1..self.values.len()).filter(|v| !preferred.contains_key(v)));
        let mut decisions: Vec<(usize, i32, bool)> = vec![];
        let mut cursor = 0;
        loop {
            while cursor < queue.len() && self.values[queue[cursor]] != 0 {
                cursor += 1;
            }
            if cursor == queue.len() {
                return Some(true);
            }
            let variable = queue[cursor];
            let literal = preferred
                .get(&variable)
                .copied()
                .unwrap_or_else(|| signed(variable));
            decisions.push((self.trail.len(), literal, false));
            self.assign(literal, Reason::Given);
            while self
                .propagate(decisions.last().expect("a decision").0)
                .is_some()
            {
                budget -= 1;
                if budget < 0 {
                    return None;
                }
                while decisions.last().is_some_and(|d| d.2) {
                    let (mark, _, _) = decisions.pop().expect("a decision");
                    self.undo(mark);
                }
                let Some((mark, literal, _)) = decisions.pop() else {
                    return Some(false);
                };
                self.undo(mark);
                decisions.push((mark, -literal, true));
                self.assign(-literal, Reason::Given);
                cursor = 0;
            }
        }
    }
}

/// A stacking: `(upper, lower)` for every pair of overlapping faces.
///
/// `fixed` lists the orders required, as `(upper, lower)`. The search tries the orders
/// in `prefer` first, so among valid stackings it finds one that keeps them where it
/// can, and gives up after `budget` dead ends.
///
/// # Errors
///
/// [`StackError::NoStacking`] when no stacking exists, with the conflict's reasons
/// when propagation alone finds it, and [`StackError::Budget`] when the search runs
/// out of budget.
pub fn stack(
    problem: &Problem<'_>,
    fixed: &[Pair],
    prefer: &[Pair],
    budget: i64,
) -> Result<Vec<Pair>, StackError> {
    let required: Vec<i32> = fixed
        .iter()
        .filter_map(|&(u, l)| problem.literal(u, l))
        .collect();
    let given: FxHashSet<i32> = required.iter().copied().collect();
    let mut contradiction: Vec<i32> = required
        .iter()
        .copied()
        .filter(|l| given.contains(&-l))
        .collect();
    if !contradiction.is_empty() {
        contradiction.sort_unstable();
        contradiction.dedup();
        return Err(StackError::NoStacking {
            constraints: vec![],
            fixed: contradiction.iter().map(|&l| problem.pair(l)).collect(),
        });
    }
    let mut solver = Solver::new(problem);
    let mut seen = FxHashSet::default();
    for &literal in &required {
        if seen.insert(literal) {
            solver.assign(literal, Reason::Given);
        }
    }
    if let Some(conflict) = solver.propagate(0) {
        let (constraints, fixed) = solver.explain(conflict);
        return Err(StackError::NoStacking { constraints, fixed });
    }
    let order: Vec<i32> = prefer
        .iter()
        .filter_map(|&(u, l)| problem.literal(u, l))
        .collect();
    match solver.search(&order, budget) {
        None => Err(StackError::Budget),
        Some(false) => Err(StackError::NoStacking {
            constraints: vec![],
            fixed: vec![],
        }),
        Some(true) => Ok((1..solver.values.len())
            .map(|v| problem.pair(signed(v) * i32::from(solver.values[v])))
            .collect()),
    }
}

/// The constraints a stacking breaks: each clause it meets with no order, and three
/// faces of a region that it stacks in a cycle, one cycle for each region.
///
/// `above` lists orders as `(upper, lower)`. A pair it leaves out takes neither order.
///
/// # Panics
///
/// When the clauses number 2³² or more.
#[must_use]
pub fn violations(problem: &Problem<'_>, above: &[Pair]) -> Vec<Constraint> {
    let given: FxHashSet<i32> = above
        .iter()
        .filter_map(|&(u, l)| problem.literal(u, l))
        .collect();
    let mut origins = BTreeSet::new();
    for index in 0..u32::try_from(problem.origins.len()).expect("clauses fit in 32 bits") {
        if !problem
            .clause(index)
            .iter()
            .any(|literal| given.contains(literal))
        {
            origins.insert(problem.origins[index as usize]);
        }
    }
    let mut found: Vec<Constraint> = origins
        .into_iter()
        .map(|origin| problem.constraints[origin as usize].constraint())
        .collect();
    let mut cycles = BTreeSet::new();
    for faces in &problem.regions {
        let beats = |f: u32, g: u32| {
            problem
                .literal(f, g)
                .is_some_and(|literal| given.contains(&literal))
        };
        if let Some(mut cycle) = cycle_among(faces, beats) {
            cycle.sort_unstable();
            cycles.insert(cycle);
        }
    }
    found.extend(cycles.into_iter().map(|faces| Constraint {
        kind: "transitivity",
        faces: faces.to_vec(),
        crease: None,
    }));
    found
}

/// Three faces that `beats` orders in a cycle, if any: depth first to some cycle, then
/// cutting a chord off it until three faces are left.
fn cycle_among(faces: &[u32], beats: impl Fn(u32, u32) -> bool) -> Option<[u32; 3]> {
    let mut state = vec![0_u8; faces.len()];
    for root in 0..faces.len() {
        if state[root] != 0 {
            continue;
        }
        let mut path = vec![(root, 0_usize)];
        state[root] = 1;
        while let Some(&mut (here, ref mut next)) = path.last_mut() {
            let Some(there) =
                (*next..faces.len()).find(|&k| beats(faces[here], faces[k]) && state[k] != 2)
            else {
                state[here] = 2;
                path.pop();
                continue;
            };
            *next = there + 1;
            if state[there] == 1 {
                let start = path
                    .iter()
                    .position(|&(k, _)| k == there)
                    .expect("on the path");
                let mut cycle: Vec<u32> = path[start..].iter().map(|&(k, _)| faces[k]).collect();
                while cycle.len() > 3 {
                    // The first face beats the second; it beats the third or the third beats it.
                    if beats(cycle[2], cycle[0]) {
                        cycle.truncate(3);
                    } else {
                        cycle.remove(1);
                    }
                }
                return (cycle.len() == 3).then(|| [cycle[0], cycle[1], cycle[2]]);
            }
            state[there] = 1;
            path.push((there, 0));
        }
    }
    None
}

/// Whether some face repeats among four.
fn repeats(faces: &[u32; 4]) -> bool {
    (0..4).any(|i| (i + 1..4).any(|j| faces[i] == faces[j]))
}

/// The pairs of creases on one line that share a stretch of positive length, by their
/// places in the group, in the order of the pairs.
fn overlapping_stretches(
    group: &[&Crease],
    line: &Line,
    tower: &crate::field::Tower,
) -> Vec<(usize, usize)> {
    let along = |p: &Point| {
        if line.normal.1.is_zero() {
            p.y.clone()
        } else {
            p.x.clone()
        }
    };
    let spans: Vec<(crate::field::Number, crate::field::Number)> = group
        .iter()
        .map(|c| {
            let (a, b) = (along(&c.ends.0), along(&c.ends.1));
            if a.compare(&b, tower).is_le() {
                (a, b)
            } else {
                (b, a)
            }
        })
        .collect();
    let mut order: Vec<usize> = (0..group.len()).collect();
    order.sort_by(|&i, &j| spans[i].0.compare(&spans[j].0, tower));
    let mut pairs = vec![];
    let mut active: Vec<usize> = vec![];
    for &i in &order {
        // A span still open reaches past this one's start, so the two share a stretch.
        active.retain(|&j| spans[i].0.compare(&spans[j].1, tower).is_lt());
        pairs.extend(active.iter().map(|&j| (i.min(j), i.max(j))));
        active.push(i);
    }
    pairs.sort_unstable();
    pairs
}
