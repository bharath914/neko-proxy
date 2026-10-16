//! View state for the flow table: a sparse page cache over the server's
//! sorted result set. Rows are fetched by page as they scroll into view;
//! live changes patch what is loaded and mark the set stale so a refresh
//! reconciles ordering. No searching or filtering happens here.

use proto::pb;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::{Duration, Instant};

pub const PAGE: usize = 200;
const REFRESH_EVERY: Duration = Duration::from_millis(120);

#[derive(Default)]
pub struct PageSet {
    pub generation: u64,
    pub total: usize,
    pub total_known: bool,
    pages: HashMap<usize, Vec<pb::FlowSummary>>,
    requested: HashSet<usize>,
    index: HashMap<u64, (usize, usize)>,
}

impl PageSet {
    fn new(generation: u64) -> Self {
        PageSet {
            generation,
            ..Default::default()
        }
    }

    pub fn get(&self, row: usize) -> Option<&pb::FlowSummary> {
        self.pages
            .get(&(row / PAGE))
            .and_then(|p| p.get(row % PAGE))
    }

    pub fn row_of(&self, id: u64) -> Option<usize> {
        self.index.get(&id).map(|(p, i)| p * PAGE + i)
    }

    fn insert(&mut self, page: usize, items: Vec<pb::FlowSummary>, total: usize) {
        self.total = total;
        self.total_known = true;
        for (i, f) in items.iter().enumerate() {
            self.index.insert(f.id, (page, i));
        }
        self.pages.insert(page, items);
    }

    fn missing(&mut self, rows: Range<usize>) -> Vec<usize> {
        let first = rows.start / PAGE;
        let last = rows.end.saturating_sub(1) / PAGE;
        let mut out = Vec::new();
        for p in first.saturating_sub(0)..=last {
            if !self.pages.contains_key(&p) && self.requested.insert(p) {
                out.push(p);
            }
        }
        out
    }
}

pub struct FlowTable {
    shown: PageSet,
    pending: Option<PageSet>,
    generation: u64,
    stale: bool,
    last_refresh: Instant,
    pub kind: i32,
    pub text: String,
}

#[derive(Default, Debug)]
pub struct BatchEffect {
    /// Ids in the batch that were already loaded and patched in place.
    pub patched: Vec<u64>,
    pub cleared: bool,
}

impl FlowTable {
    pub fn new() -> Self {
        FlowTable {
            shown: PageSet::default(),
            pending: None,
            generation: 0,
            stale: true,
            last_refresh: Instant::now() - Duration::from_secs(1),
            kind: 0,
            text: String::new(),
        }
    }

    pub fn total(&self) -> usize {
        self.shown.total
    }

    pub fn loaded(&self) -> bool {
        self.shown.total_known
    }

    pub fn get(&self, row: usize) -> Option<&pb::FlowSummary> {
        self.shown.get(row)
    }

    pub fn row_of(&self, id: u64) -> Option<usize> {
        self.shown.row_of(id)
    }

    /// Start a fresh result set (filter changed, reconnect, resync). The
    /// old rows stay on screen until the new set can replace them.
    pub fn invalidate(&mut self) {
        self.generation += 1;
        self.pending = Some(PageSet::new(self.generation));
        self.stale = false;
        self.last_refresh = Instant::now();
    }

    /// Pages to request for the visible `rows`, as (generation, page).
    pub fn wanted(&mut self, rows: Range<usize>) -> Vec<(u64, usize)> {
        if self.stale && self.last_refresh.elapsed() >= REFRESH_EVERY && self.pending.is_none() {
            self.invalidate();
        }
        let set = match self.pending.as_mut() {
            Some(p) => p,
            None => &mut self.shown,
        };
        let gen = set.generation;
        // Always know the total: page 0 of a new set is the cheap way.
        let mut rows = rows;
        if !set.total_known && rows.is_empty() {
            rows = 0..1;
        }
        let mut pages = set.missing(rows.clone());
        // Prefetch the next page while scrolling.
        if set.total_known {
            let next = rows.end / PAGE + 1;
            if next * PAGE < set.total
                && set.requested.insert(next)
                && !set.pages.contains_key(&next)
            {
                pages.push(next);
            }
        }
        pages.into_iter().map(|p| (gen, p)).collect()
    }

    pub fn on_page(&mut self, gen: u64, page: usize, resp: pb::SearchResponse) {
        let total = resp.total as usize;
        if let Some(p) = self.pending.as_mut() {
            if p.generation == gen {
                p.insert(page, resp.items, total);
                // Promote once the first window has landed.
                if p.pages.contains_key(&page) {
                    let done = self.pending.take().unwrap();
                    // Keep pages the old set already had if the totals match.
                    self.shown = done;
                }
                return;
            }
        }
        if self.shown.generation == gen {
            self.shown.insert(page, resp.items, total);
        }
    }

    pub fn on_page_failed(&mut self, gen: u64, page: usize) {
        for set in [self.pending.as_mut(), Some(&mut self.shown)]
            .into_iter()
            .flatten()
        {
            if set.generation == gen {
                set.requested.remove(&page);
            }
        }
    }

    /// Apply a pushed batch: patch loaded rows, and note whether a refresh is
    /// needed for rows we have not seen.
    pub fn apply_batch(&mut self, b: &pb::FlowBatch) -> BatchEffect {
        let mut effect = BatchEffect::default();
        if b.cleared {
            self.shown = PageSet::default();
            self.pending = None;
            self.generation += 1;
            self.shown.generation = self.generation;
            self.stale = true;
            effect.cleared = true;
        }
        // A row that stopped being a row of its own (merged into another).
        let mut unknown = !b.removed.is_empty();
        for u in &b.upserts {
            match self.shown.index.get(&u.id).copied() {
                Some((p, i)) => {
                    if let Some(slot) = self.shown.pages.get_mut(&p).and_then(|v| v.get_mut(i)) {
                        *slot = u.clone();
                        effect.patched.push(u.id);
                    }
                }
                None => unknown = true,
            }
        }
        if unknown
            || (b.total as usize != self.shown.total
                && self.shown.total_known
                && self.text.is_empty()
                && self.kind == 0)
        {
            self.stale = true;
        }
        effect
    }
}

impl Default for FlowTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: u64) -> pb::FlowSummary {
        pb::FlowSummary {
            id,
            ..Default::default()
        }
    }

    fn page(range: Range<u64>, total: u64) -> pb::SearchResponse {
        pb::SearchResponse {
            total,
            offset: 0,
            items: range.map(summary).collect(),
        }
    }

    #[test]
    fn first_window_requests_page_zero_then_promotes() {
        let mut t = FlowTable::new();
        t.invalidate();
        let want = t.wanted(0..30);
        assert_eq!(want, vec![(1, 0)]);
        // Same window again requests nothing new.
        assert!(t.wanted(0..30).is_empty());
        t.on_page(1, 0, page(1..201, 450));
        assert_eq!(t.total(), 450);
        assert_eq!(t.get(5).unwrap().id, 6);
        assert_eq!(t.row_of(10), Some(9));
    }

    #[test]
    fn scrolling_fetches_further_pages() {
        let mut t = FlowTable::new();
        t.invalidate();
        t.wanted(0..30);
        t.on_page(1, 0, page(1..201, 1000));
        let want = t.wanted(190..230);
        assert!(want.contains(&(1, 1)));
        t.on_page(1, 1, page(201..401, 1000));
        assert_eq!(t.get(250).unwrap().id, 251);
    }

    #[test]
    fn batch_patches_known_rows_and_flags_unknown() {
        let mut t = FlowTable::new();
        t.invalidate();
        t.wanted(0..30);
        t.on_page(1, 0, page(1..11, 10));
        let mut upd = summary(3);
        upd.status = 404;
        let e = t.apply_batch(&pb::FlowBatch {
            upserts: vec![upd],
            cleared: false,
            total: 10,
            removed: vec![],
        });
        assert_eq!(e.patched, vec![3]);
        assert_eq!(t.get(2).unwrap().status, 404);
        assert!(!t.stale);
        t.apply_batch(&pb::FlowBatch {
            upserts: vec![summary(99)],
            cleared: false,
            total: 11,
            removed: vec![],
        });
        assert!(t.stale);
    }

    #[test]
    fn stale_generation_is_ignored() {
        let mut t = FlowTable::new();
        t.invalidate();
        t.wanted(0..30);
        t.invalidate(); // filter changed before the first reply
        t.on_page(1, 0, page(1..11, 10));
        assert_eq!(t.total(), 0);
        let want = t.wanted(0..30);
        assert_eq!(want, vec![(2, 0)]);
    }

    #[test]
    fn clear_resets_everything() {
        let mut t = FlowTable::new();
        t.invalidate();
        t.wanted(0..30);
        t.on_page(1, 0, page(1..11, 10));
        let e = t.apply_batch(&pb::FlowBatch {
            upserts: vec![],
            cleared: true,
            total: 0,
            removed: vec![],
        });
        assert!(e.cleared);
        assert_eq!(t.total(), 0);
    }
}
