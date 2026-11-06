//! The layer solver: which face lies above which, wherever two faces overlap, as
//! `origamy.layers` defines it.
//!
//! Each pair of overlapping faces is a variable: positive puts the lesser face above.
//! Folds and flat creases give parity rules: an even number of a rule's literals hold.
//! A taco-taco rule has four literals, and the others have two, which then agree.
//!
//! Transitivity forbids any three faces that share area from stacking in a cycle. Three
//! faces share area exactly when they meet in a region of the footprints' overlay where
//! the most footprints meet. Each such region keeps the orders among its faces as rows
//! of bits. An order that holds marks every region holding its two faces at once, and
//! each region closes its new orders under transitivity a row of bits at a time. The
//! solver looks up the literal of a closed order only when no region knew it.

#![allow(
    clippy::many_single_char_names,
    reason = "faces f, g and h, and literals x and y, follow the papers' names"
)]

use std::collections::BTreeSet;

use rayon::prelude::*;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::folded::{Folded, Pair};
use crate::geometry::{Line, Point};

/// One geometric reason that limits how some faces stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    /// `taco-taco`, `taco-tortilla`, `tortilla-tortilla` or `transitivity`.
    pub kind: &'static str,
    /// The faces involved.
    pub faces: Vec<u32>,
    /// The crease the constraint is about, by its index among the paper's creases.
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

/// A rule as the problem builds it: its constraint, and its literals, then 0s.
type Rule = (Origin, [i32; 4]);

/// The rule that two literals agree, when both exist and differ: an even number of them
/// hold.
fn equal(origin: Origin, x: Option<i32>, y: Option<i32>) -> Option<Rule> {
    let (x, y) = (x?, y?);
    (x != y).then_some((origin, [x, y, 0, 0]))
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

/// The stacking constraints of folded paper: parity rules, and transitivity found on
/// demand.
pub struct Problem<'a> {
    paper: &'a Folded,
    /// Where each face's neighbors start in `neighbors`.
    rows: Vec<u32>,
    /// Each face's overlapping faces, sorted, each with the literal that puts the face
    /// above it.
    neighbors: Vec<(u32, i32)>,
    /// Each rule's literals, then 0s.
    rules: Vec<[i32; 4]>,
    /// Each rule's constraint.
    constraints: Vec<Origin>,
    /// The faces of each region where the most footprints meet, footprint by footprint
    /// in order, each footprint's faces as it stacks them.
    regions: Vec<Vec<u32>>,
    /// Each face's place among its footprint's faces.
    rank: Vec<u32>,
    /// Each pair's bucket, by its variable: the regions its two footprints share, or
    /// `u32::MAX` when they share none.
    bucket: Vec<u32>,
    /// Where each bucket starts in `buckets`.
    bucket_starts: Vec<u32>,
    /// Each bucket's regions, with where each footprint's faces start among the
    /// region's faces, the footprint of lesser index first.
    buckets: Vec<[u32; 3]>,
}

impl<'a> Problem<'a> {
    /// Every constraint of a paper's geometry.
    ///
    /// # Panics
    ///
    /// When the constraints or their literals number 2³² or more.
    #[must_use]
    pub fn new(paper: &'a Folded) -> Problem<'a> {
        let mut rows = vec![0u32; paper.folded.len() + 1];
        for &(f, g) in &paper.overlapping {
            rows[f as usize + 1] += 1;
            rows[g as usize + 1] += 1;
        }
        for f in 0..paper.folded.len() {
            rows[f + 1] += rows[f];
        }
        let mut fill: Vec<u32> = rows.clone();
        let mut neighbors = vec![(0, 0); 2 * paper.overlapping.len()];
        for (v, &(f, g)) in paper.overlapping.iter().enumerate() {
            let literal = signed(v + 1);
            neighbors[fill[f as usize] as usize] = (g, literal);
            fill[f as usize] += 1;
            neighbors[fill[g as usize] as usize] = (f, -literal);
            fill[g as usize] += 1;
        }
        // Each face sorts its neighbors on its own core.
        let mut rest = &mut neighbors[..];
        let mut lists = Vec::with_capacity(paper.folded.len());
        for f in 0..paper.folded.len() {
            let (list, tail) = rest.split_at_mut((rows[f + 1] - rows[f]) as usize);
            lists.push(list);
            rest = tail;
        }
        lists
            .par_iter_mut()
            .for_each(|list| list.sort_unstable_by_key(|&(g, _)| g));
        let covers: Vec<Vec<u32>> = paper
            .overlay
            .regions
            .iter()
            .map(|cover| {
                let mut cover = cover.clone();
                cover.sort_unstable();
                cover
            })
            .filter(|cover| {
                cover
                    .iter()
                    .map(|&k| paper.stacked[k as usize].len())
                    .sum::<usize>()
                    > 2
            })
            .collect();
        let regions: Vec<Vec<u32>> = covers
            .iter()
            .map(|cover| {
                cover
                    .iter()
                    .flat_map(|&k| paper.stacked[k as usize].iter().copied())
                    .collect()
            })
            .collect();
        let mut rank = vec![0; paper.folded.len()];
        for faces in &paper.stacked {
            for (k, &f) in faces.iter().enumerate() {
                rank[f as usize] = u32::try_from(k).expect("faces fit in 32 bits");
            }
        }
        let (bucket, bucket_starts, buckets) = buckets(paper, &covers, &rows, &neighbors);
        let mut problem = Problem {
            paper,
            rows,
            neighbors,
            rules: vec![],
            constraints: vec![],
            regions,
            rank,
            bucket,
            bucket_starts,
            buckets,
        };
        let creases = problem.creases();
        let (mut constraints, mut rules) = problem.folds_and_flats(&creases);
        let (crossing, crossed) = problem.crossings(&creases);
        constraints.extend(crossing);
        rules.extend(crossed);
        problem.constraints = constraints;
        problem.rules = rules;
        problem
    }

    /// How many pairs of faces overlap: the variables.
    #[must_use]
    pub fn variables(&self) -> usize {
        self.paper.overlapping.len()
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
        let (f, g) = self.paper.overlapping[literal.unsigned_abs() as usize - 1];
        if literal > 0 { (f, g) } else { (g, f) }
    }

    /// A rule's literals, by index: an even number of them hold.
    fn rule(&self, index: u32) -> &[i32] {
        let literals = &self.rules[index as usize];
        &literals[..literals.iter().position(|&l| l == 0).unwrap_or(4)]
    }

    fn creases(&self) -> Vec<Crease> {
        let paper = self.paper;
        paper
            .creases
            .par_iter()
            .enumerate()
            .map(|(index, &([v, w], [f, g]))| {
                let ends = (paper.image(f, v).clone(), paper.image(f, w).clone());
                let line = paper.overlay.crease_lines[index];
                let side = paper.inside(f, v, w);
                let index = u32::try_from(index).expect("creases fit in 32 bits");
                if paper.same_place(f, g) {
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

    /// The rules of creases that share a stretch of a line: each line finds its own on
    /// every core, and the rules come line by line.
    fn folds_and_flats(&self, creases: &[Crease]) -> (Vec<Origin>, Vec<[i32; 4]>) {
        let tower = &self.paper.tower;
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
        groups
            .par_iter()
            .flat_map_iter(|group| {
                let line = &self.paper.overlay.arrangement.lines[group[0].line as usize];
                let mut rules = vec![];
                for (i, j) in overlapping_stretches(group, line, tower) {
                    let (first, second) = (group[i], group[j]);
                    let rule = if first.side != 0 && second.side != 0 {
                        if first.side == second.side {
                            self.taco_taco(first, second)
                        } else {
                            None
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
                        self.taco_tortilla(taco, tortilla)
                    } else {
                        self.tortilla_tortilla(first, second)
                    };
                    rules.extend(rule);
                }
                rules
            })
            .unzip()
    }

    fn taco_taco(&self, first: &Crease, second: &Crease) -> Option<Rule> {
        let ([a, b], [c, d]) = (first.faces, second.faces);
        if repeats(&[a, b, c, d]) {
            return None;
        }
        let literals = [
            self.literal(c, a)?,
            self.literal(c, b)?,
            self.literal(d, a)?,
            self.literal(d, b)?,
        ];
        // c lies between a and b when exactly one of a and b is below it; so does d.
        // Nesting or stacking puts both or neither between: the parity is even.
        Some((
            Origin::new("taco-taco", &[a, b, c, d], first.index),
            literals,
        ))
    }

    fn taco_tortilla(&self, taco: &Crease, tortilla: u32) -> Option<Rule> {
        let [a, b] = taco.faces;
        if tortilla == a || tortilla == b {
            return None;
        }
        let origin = Origin::new("taco-tortilla", &[a, b, tortilla], taco.index);
        equal(origin, self.literal(tortilla, a), self.literal(tortilla, b))
    }

    fn tortilla_tortilla(&self, first: &Crease, second: &Crease) -> Option<Rule> {
        let ([p1, n1], [p2, n2]) = (first.faces, second.faces);
        if repeats(&[p1, n1, p2, n2]) {
            return None;
        }
        let origin = Origin::new("tortilla-tortilla", &[p1, n1, p2, n2], first.index);
        equal(origin, self.literal(p1, p2), self.literal(n1, n2))
    }

    /// The rules of creases that run through footprints, crease by crease, each crease's
    /// found on every core.
    fn crossings(&self, creases: &[Crease]) -> (Vec<Origin>, Vec<[i32; 4]>) {
        let paper = self.paper;
        creases
            .par_iter()
            .flat_map_iter(|crease| {
                let [a, b] = crease.faces;
                let kind = if crease.side != 0 {
                    "taco-tortilla"
                } else {
                    "tortilla-tortilla"
                };
                let mut rules = vec![];
                for &k in &paper.overlay.crossed[crease.index as usize] {
                    for &face in &paper.stacked[k as usize] {
                        if face != a && face != b {
                            let origin = Origin::new(kind, &[a, b, face], crease.index);
                            rules.extend(equal(
                                origin,
                                self.literal(face, a),
                                self.literal(face, b),
                            ));
                        }
                    }
                }
                rules
            })
            .unzip()
    }

    /// The regions holding both faces of a pair, by its variable, each with the faces'
    /// places among its faces, `upper` first.
    fn shared(&self, variable: usize, (upper, lower): Pair, out: &mut Vec<(u32, u32, u32)>) {
        out.clear();
        let index = self.bucket[variable - 1];
        if index == u32::MAX {
            return;
        }
        let (u, l) = (upper as usize, lower as usize);
        let first = self.paper.footprint[u] <= self.paper.footprint[l];
        let (ru, rl) = (self.rank[u], self.rank[l]);
        let entries = &self.buckets[self.bucket_starts[index as usize] as usize
            ..self.bucket_starts[index as usize + 1] as usize];
        out.extend(entries.iter().map(|&[r, a, b]| {
            let (at_u, at_l) = if first { (a, b) } else { (b, a) };
            (r, at_u + ru, at_l + rl)
        }));
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
    /// A rule, by index.
    Rule(u32),
    /// A cycle of three faces, f < g < h.
    Cycle([u32; 3]),
}

/// Unit propagation and chronological backtracking over a problem's constraints.
struct Solver<'p, 'a> {
    problem: &'p Problem<'a>,
    values: Vec<i8>,
    reasons: Vec<Reason>,
    trail: Vec<i32>,
    /// Where each variable's watches start in `watches`.
    watch_starts: Vec<u32>,
    /// The rules each variable appears in, variable by variable, in the rules' order.
    watches: Vec<Watch>,
    orders: Vec<Orders>,
    /// The regions with orders known but not closed yet, and whether each region is.
    dirty: Vec<u32>,
    is_dirty: Vec<bool>,
    scratch: Vec<(u32, u32, u32)>,
    lowers: Vec<u64>,
    raised: Vec<u64>,
}

/// A rule as a variable in it watches it: its index, and its other literals, then 0s.
/// The first takes the variable's sign, so that an even number of them and of the
/// variable's positive literal hold.
#[derive(Clone, Copy)]
struct Watch {
    index: u32,
    others: [i32; 3],
}

/// The orders among a region's faces, as bits. Row `x` of `known` holds the faces below
/// face `x` whose orders hold; `below` and `above` hold those closed under transitivity.
/// The two agree once propagation ends.
struct Orders {
    words: usize,
    known: Vec<u64>,
    above: Vec<u64>,
    below: Vec<u64>,
}

impl Orders {
    fn new(size: usize) -> Orders {
        let words = size.div_ceil(64);
        Orders {
            words,
            known: vec![0; size * words],
            above: vec![0; size * words],
            below: vec![0; size * words],
        }
    }

    fn holds(&self, x: usize, y: usize) -> bool {
        self.below[x * self.words + y / 64] >> (y % 64) & 1 == 1
    }

    fn clear(&mut self, x: usize, y: usize) {
        let bit = !(1 << (y % 64));
        self.known[x * self.words + y / 64] &= bit;
        self.below[x * self.words + y / 64] &= bit;
        self.above[y * self.words + x / 64] &= !(1 << (x % 64));
    }
}

/// The set bits of some words, in increasing order.
struct Bits<'w> {
    words: &'w [u64],
    word: usize,
    rest: u64,
}

impl<'w> Bits<'w> {
    fn new(words: &'w [u64]) -> Bits<'w> {
        Bits {
            words,
            word: 0,
            rest: words.first().copied().unwrap_or(0),
        }
    }
}

impl Iterator for Bits<'_> {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        while self.rest == 0 {
            self.word += 1;
            self.rest = *self.words.get(self.word)?;
        }
        let bit = self.rest.trailing_zeros() as usize;
        self.rest &= self.rest - 1;
        Some(self.word * 64 + bit)
    }
}

/// A variable as a positive literal.
fn signed(variable: usize) -> i32 {
    i32::try_from(variable).expect("variables fit in 31 bits")
}

impl<'p, 'a> Solver<'p, 'a> {
    fn new(problem: &'p Problem<'a>) -> Self {
        let variables = problem.variables() + 1;
        let mut starts = vec![0u32; variables + 1];
        for literal in problem.rules.iter().flatten().filter(|&&l| l != 0) {
            starts[literal.unsigned_abs() as usize + 1] += 1;
        }
        for v in 0..variables {
            starts[v + 1] += starts[v];
        }
        let mut fill = starts.clone();
        let mut watches = vec![
            Watch {
                index: 0,
                others: [0; 3]
            };
            starts[variables] as usize
        ];
        for index in 0..u32::try_from(problem.constraints.len()).expect("rules fit in 32 bits") {
            let rule = problem.rule(index);
            for (k, &literal) in rule.iter().enumerate() {
                let mut others = [0; 3];
                let rest = rule[..k].iter().chain(&rule[k + 1..]);
                for (slot, &other) in others.iter_mut().zip(rest) {
                    *slot = other;
                }
                // The first other literal takes the variable's sign, so that the variable's
                // positive literal stands in its place.
                others[0] *= literal.signum();
                let v = literal.unsigned_abs() as usize;
                watches[fill[v] as usize] = Watch { index, others };
                fill[v] += 1;
            }
        }
        Solver {
            problem,
            values: vec![0; variables],
            reasons: vec![Reason::Given; variables],
            trail: vec![],
            watch_starts: starts,
            watches,
            orders: problem
                .regions
                .iter()
                .map(|faces| Orders::new(faces.len()))
                .collect(),
            dirty: vec![],
            is_dirty: vec![false; problem.regions.len()],
            scratch: vec![],
            lowers: vec![],
            raised: vec![],
        }
    }

    fn value(&self, literal: i32) -> i8 {
        let value = self.values[literal.unsigned_abs() as usize];
        if literal > 0 { value } else { -value }
    }

    /// Make a literal hold, and mark its order in every region holding its two faces.
    fn assign(&mut self, literal: i32, reason: Reason) {
        let variable = literal.unsigned_abs() as usize;
        self.values[variable] = if literal > 0 { 1 } else { -1 };
        self.reasons[variable] = reason;
        self.trail.push(literal);
        let mut regions = std::mem::take(&mut self.scratch);
        self.problem
            .shared(variable, self.problem.pair(literal), &mut regions);
        for &(r, u, l) in &regions {
            let orders = &mut self.orders[r as usize];
            orders.known[u as usize * orders.words + l as usize / 64] |= 1 << (l % 64);
            if !self.is_dirty[r as usize] {
                self.is_dirty[r as usize] = true;
                self.dirty.push(r);
            }
        }
        self.scratch = regions;
    }

    /// Deduce from the trail after `start` and the orders known but not closed; return
    /// a constraint that fails, if any.
    fn propagate(&mut self, start: usize) -> Option<Reason> {
        let mut head = start;
        loop {
            while head < self.trail.len() {
                let variable = self.trail[head].unsigned_abs() as usize;
                head += 1;
                if let Some(conflict) = self.rules(variable) {
                    return Some(conflict);
                }
            }
            let r = self.dirty.pop()?;
            let r = r as usize;
            self.is_dirty[r] = false;
            let words = self.orders[r].words;
            for u in 0..self.problem.regions[r].len() {
                for w in 0..words {
                    let at = u * words + w;
                    let orders = &self.orders[r];
                    let mut open = orders.known[at] & !orders.below[at];
                    while open != 0 {
                        let l = w * 64 + open.trailing_zeros() as usize;
                        open &= open - 1;
                        if let Some(conflict) = self.close(r, u, l) {
                            return Some(conflict);
                        }
                    }
                }
            }
        }
    }

    /// Propagation through the rules that a variable just assigned appears in: a rule
    /// with one literal open sets it to make the parity even.
    fn rules(&mut self, variable: usize) -> Option<Reason> {
        let own = self.values[variable] > 0;
        let (from, to) = (
            self.watch_starts[variable] as usize,
            self.watch_starts[variable + 1] as usize,
        );
        for k in from..to {
            let Watch { index, others } = self.watches[k];
            let mut open = None;
            let mut odd = own;
            let mut several = false;
            for &literal in others.iter().take_while(|&&l| l != 0) {
                match self.value(literal) {
                    0 => {
                        several = open.is_some();
                        if several {
                            break;
                        }
                        open = Some(literal);
                    }
                    value => odd ^= value > 0,
                }
            }
            if several {
                continue;
            }
            match open {
                None if odd => return Some(Reason::Rule(index)),
                None => {}
                Some(literal) => {
                    let literal = if odd { literal } else { -literal };
                    self.assign(literal, Reason::Rule(index));
                }
            }
        }
        None
    }

    /// Close a region's orders under a known one, face `u` above face `l` by their
    /// places; return a cycle that closes, if any.
    ///
    /// The closed orders hold, so a new order that closes a cycle shows as a literal
    /// already false. Every other new order is either known or a new literal.
    fn close(&mut self, r: usize, u: usize, l: usize) -> Option<Reason> {
        if self.orders[r].holds(u, l) {
            return None;
        }
        let problem = self.problem;
        let faces = &problem.regions[r];
        let words = self.orders[r].words;
        // Every face at or above `u` and not yet above `l` goes above every face at or
        // below `l` and not yet below `u`; the other orders hold already.
        let mut lowers = std::mem::take(&mut self.lowers);
        let mut raised = std::mem::take(&mut self.raised);
        lowers.clear();
        raised.clear();
        {
            let orders = &self.orders[r];
            let (row_l, row_u) = (l * words, u * words);
            lowers.extend_from_slice(&orders.below[row_l..row_l + words]);
            lowers[l / 64] |= 1 << (l % 64);
            raised.extend_from_slice(&orders.above[row_u..row_u + words]);
            raised[u / 64] |= 1 << (u % 64);
            for w in 0..words {
                lowers[w] &= !orders.below[row_u + w];
                raised[w] &= !orders.above[row_l + w];
            }
        }
        let (upper, lower) = (faces[u], faces[l]);
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
        // Face `u` comes first: each other face's new orders cite `u`'s, which must hold
        // before them.
        let rest = Bits::new(&raised).filter(|&x| x != u);
        'uppers: for x in std::iter::once(u).chain(rest) {
            let row = x * words;
            for (w, &low) in lowers.iter().enumerate() {
                let orders = &mut self.orders[r];
                let new = low & !orders.below[row + w];
                orders.below[row + w] |= new;
                let mut unknown = new & !orders.known[row + w];
                while unknown != 0 {
                    let y = w * 64 + unknown.trailing_zeros() as usize;
                    unknown &= unknown - 1;
                    let literal = problem
                        .literal(faces[x], faces[y])
                        .expect("faces of a region overlap");
                    if self.value(literal) == 0 {
                        self.assign(literal, Reason::Cycle(triple(x, y)));
                    } else {
                        // Every order set must hold for undoing to clear it: drop the
                        // ones not reached.
                        self.orders[r].below[row + w] &= !(unknown | 1 << (y % 64));
                        found = Some(Reason::Cycle(triple(x, y)));
                        break 'uppers;
                    }
                }
            }
        }
        if found.is_none() {
            let above = &mut self.orders[r].above;
            for y in Bits::new(&lowers) {
                for (word, &bits) in above[y * words..(y + 1) * words].iter_mut().zip(&raised) {
                    *word |= bits;
                }
            }
        }
        self.lowers = lowers;
        self.raised = raised;
        found
    }

    fn literals(&self, reason: Reason) -> Vec<i32> {
        match reason {
            Reason::Given => vec![],
            Reason::Rule(index) => self.problem.rule(index).to_vec(),
            Reason::Cycle(faces) => self.problem.cycle(faces).to_vec(),
        }
    }

    /// The constraints and required literals behind a conflict at the root.
    fn explain(&self, conflict: Reason) -> (Vec<Constraint>, Vec<Pair>) {
        let mut rules = BTreeSet::new();
        let mut cycles = BTreeSet::new();
        let mut required = BTreeSet::new();
        let mut seen = FxHashSet::default();
        let mut pending = vec![conflict];
        while let Some(reason) = pending.pop() {
            match reason {
                Reason::Rule(index) => {
                    rules.insert(index);
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
                debug_assert!(
                    self.values[variable] != 0,
                    "a reason cites an open variable"
                );
                match self.reasons[variable] {
                    Reason::Given => {
                        required.insert(signed(variable) * i32::from(self.values[variable]));
                    }
                    reason => pending.push(reason),
                }
            }
        }
        let problem = self.problem;
        let mut constraints: Vec<Constraint> = rules
            .iter()
            .map(|&index| problem.constraints[index as usize].constraint())
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
            let pair = self.problem.paper.overlapping[variable - 1];
            self.problem.shared(variable, pair, &mut regions);
            for &(r, a, b) in &regions {
                let (a, b) = (a as usize, b as usize);
                self.orders[r as usize].clear(a, b);
                self.orders[r as usize].clear(b, a);
            }
        }
        self.trail.truncate(mark);
        self.scratch = regions;
        // Orders known but not closed past a conflict belong to literals just undone.
        for r in self.dirty.drain(..) {
            self.is_dirty[r as usize] = false;
        }
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

/// The constraints a stacking breaks: each rule with a pair it leaves out or an odd
/// number of literals that hold, and three faces of a region that it stacks in a cycle,
/// one cycle for each region.
///
/// `above` lists orders as `(upper, lower)`. A pair it leaves out takes neither order,
/// and a pair it lists both ways breaks no rule.
///
/// # Panics
///
/// When the rules number 2³² or more.
#[must_use]
pub fn violations(problem: &Problem<'_>, above: &[Pair]) -> Vec<Constraint> {
    let given: FxHashSet<i32> = above
        .iter()
        .filter_map(|&(u, l)| problem.literal(u, l))
        .collect();
    let mut found: Vec<Constraint> = vec![];
    for index in 0..u32::try_from(problem.constraints.len()).expect("rules fit in 32 bits") {
        let mut odd = false;
        let mut open = false;
        let mut both = false;
        for literal in problem.rule(index) {
            match (given.contains(literal), given.contains(&-literal)) {
                (true, true) => both = true,
                (true, false) => odd = !odd,
                (false, false) => open = true,
                (false, true) => {}
            }
        }
        if !both && (open || odd) {
            found.push(problem.constraints[index as usize].constraint());
        }
    }
    let mut cycles = BTreeSet::new();
    for region in &problem.regions {
        let mut faces = region.clone();
        faces.sort_unstable();
        let faces = &faces;
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
    sorted_pairs(&pairs, group.len())
}

/// Pairs of indices below `size`, sorted: a counting sort by the second, then a stable
/// one by the first.
fn sorted_pairs(pairs: &[(usize, usize)], size: usize) -> Vec<(usize, usize)> {
    let mut order = pairs.to_vec();
    for key in [|p: &(usize, usize)| p.1, |p: &(usize, usize)| p.0] {
        let mut starts = vec![0usize; size + 1];
        for p in pairs {
            starts[key(p) + 1] += 1;
        }
        for k in 0..size {
            starts[k + 1] += starts[k];
        }
        let from = order.clone();
        for p in &from {
            order[starts[key(p)]] = *p;
            starts[key(p)] += 1;
        }
    }
    order
}

/// The regions that faces share, in a bucket for each pair of their footprints: each
/// pair of overlapping faces' bucket, where each bucket starts, and the buckets. An entry
/// is a region and where each footprint's faces start among its faces.
fn buckets(
    paper: &Folded,
    covers: &[Vec<u32>],
    rows: &[u32],
    neighbors: &[(u32, i32)],
) -> (Vec<u32>, Vec<u32>, Vec<[u32; 3]>) {
    let count = paper.stacked.len();
    let index = |n: usize| u32::try_from(n).expect("entries fit in 32 bits");
    // Each footprint's regions, with where its faces start among each region's faces.
    let mut starts = vec![0u32; count + 1];
    for cover in covers {
        for &k in cover {
            starts[k as usize + 1] += 1;
        }
    }
    for k in 0..count {
        starts[k + 1] += starts[k];
    }
    let mut fill = starts.clone();
    let mut holding = vec![(0u32, 0u32); starts[count] as usize];
    let mut offsets: Vec<Vec<u32>> = Vec::with_capacity(covers.len());
    for (r, cover) in covers.iter().enumerate() {
        let mut at = 0;
        let mut offset = Vec::with_capacity(cover.len());
        for &k in cover {
            holding[fill[k as usize] as usize] = (index(r), at);
            fill[k as usize] += 1;
            offset.push(at);
            at += index(paper.stacked[k as usize].len());
        }
        offsets.push(offset);
    }
    let mut bucket = vec![u32::MAX; paper.overlapping.len()];
    let mut starts_of = vec![0u32];
    let mut entries: Vec<[u32; 3]> = vec![];
    // Each footprint's partners of no lesser index, by the order they first appear.
    let mut slot = vec![u32::MAX; count];
    let mut partners: Vec<u32> = vec![];
    let mut sizes: Vec<u32> = vec![];
    for k in 0..count {
        let regions = &holding[starts[k] as usize..starts[k + 1] as usize];
        partners.clear();
        sizes.clear();
        for &(r, _) in regions {
            let cover = &covers[r as usize];
            for &m in &cover[cover.partition_point(|&m| (m as usize) < k)..] {
                if slot[m as usize] == u32::MAX {
                    slot[m as usize] = index(partners.len());
                    partners.push(m);
                    sizes.push(0);
                }
                sizes[slot[m as usize] as usize] += 1;
            }
        }
        let base = index(starts_of.len() - 1);
        let first = entries.len();
        let mut fill: Vec<usize> = Vec::with_capacity(sizes.len());
        let mut end = first;
        for &size in &sizes {
            fill.push(end);
            end += size as usize;
            starts_of.push(index(end));
        }
        entries.resize(end, [0; 3]);
        for &(r, at) in regions {
            let cover = &covers[r as usize];
            let from = cover.partition_point(|&m| (m as usize) < k);
            for (j, &m) in cover.iter().enumerate().skip(from) {
                let s = slot[m as usize] as usize;
                entries[fill[s]] = [r, at, offsets[r as usize][j]];
                fill[s] += 1;
            }
        }
        for &f in &paper.stacked[k] {
            for &(g, literal) in
                &neighbors[rows[f as usize] as usize..rows[f as usize + 1] as usize]
            {
                // A pair within the footprint, or with one of greater index: either way
                // its bucket is this footprint's.
                let m = paper.footprint[g as usize];
                if slot[m as usize] != u32::MAX {
                    bucket[literal.unsigned_abs() as usize - 1] = base + slot[m as usize];
                }
            }
        }
        for &m in &partners {
            slot[m as usize] = u32::MAX;
        }
    }
    (bucket, starts_of, entries)
}
