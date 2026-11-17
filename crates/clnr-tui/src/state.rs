//! Everything the TUI knows, as plain data. Mutated only by [`crate::update`].

use std::cmp::{Ordering, Reverse};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::SystemTime;

use clnr_core::scan::Measurement;
use clnr_core::{Artifact, ArtifactId, Safety, human};
use ratatui::layout::Rect;
use ratatui::widgets::TableState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Idle,
    Deleting,
    Failed,
}

pub struct Item {
    pub artifact: Artifact,
    pub status: Status,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    Size,
    Age,
    Name,
    Provider,
}

impl SortKey {
    pub fn next(self) -> Self {
        match self {
            Self::Size => Self::Age,
            Self::Age => Self::Name,
            Self::Name => Self::Provider,
            Self::Provider => Self::Size,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Age => "age",
            Self::Name => "name",
            Self::Provider => "provider",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// One row per artifact.
    Flat,
    /// One row per project (or per provider for unowned global caches).
    Projects,
}

/// What a [`Group`] collects.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupKey {
    Project(PathBuf),
    /// Global caches not tied to a project.
    Global(&'static str),
}

/// A row in the projects view.
pub struct Group {
    pub key: GroupKey,
    /// The project's path, or `cargo caches` for unowned globals.
    pub name: String,
    pub members: Vec<ArtifactId>,
    /// Distinct kinds inside (`target`, `node_modules`), in row order.
    pub kinds: Vec<String>,
    pub total: u64,
    pub newest: Option<SystemTime>,
    /// The project directory is gone.
    pub orphaned: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Sidebar,
    Table,
}

pub enum Mode {
    Normal,
    /// Typing a filter query.
    Filter,
    Confirm(Confirm),
    Help,
    Errors,
}

/// A planned delete waiting for the user's answer.
pub struct Confirm {
    pub allowed: Vec<ArtifactId>,
    pub blocked: Vec<(ArtifactId, String)>,
    pub total: u64,
    /// Caution items are included: the user must type `yes`.
    pub needs_yes: bool,
    pub typed: String,
}

pub struct State {
    /// (id, name) of every provider, in display order.
    pub providers: Vec<(&'static str, &'static str)>,
    pub home: std::path::PathBuf,
    items: Vec<Item>,
    index: HashMap<ArtifactId, usize>,
    /// Visible items (indices into `items`), filtered and sorted.
    rows: Vec<usize>,
    /// Visible rows of the projects view, built from `rows`.
    groups: Vec<Group>,
    dirty: bool,

    pub cursor: usize,
    /// The user moved the cursor: keep it on the same item as rows re-sort. Until then it stays
    /// on the top row (the biggest item), however the list changes while scanning.
    pub moved: bool,
    pub table: TableState,
    /// Where table rows and the sidebar are on screen; set while drawing, used for clicks.
    pub rows_area: Rect,
    pub sidebar_area: Rect,
    /// Rows that fit in the table; set while drawing.
    pub page: usize,
    pub selected: HashSet<ArtifactId>,
    pub sort: SortKey,
    pub reverse: bool,
    pub query: String,
    /// The provider picked in the sidebar; `None` is "All".
    pub provider: Option<&'static str>,
    pub focus: Focus,
    pub mode: Mode,
    pub detail: bool,
    pub view: View,

    pub scanning: bool,
    pub deleting: bool,
    /// Deletes interrupted in an earlier run, finished with the next confirmed delete.
    pub interrupted: usize,
    /// A re-measure batch is running.
    pub remeasuring: bool,
    /// Re-measure polls so far.
    pub polls: u64,
    pub errors: Vec<String>,
    pub toast: Option<String>,
    pub spinner: usize,
    /// Set by the first ctrl-c while deleting; a second one quits anyway.
    pub force_quit_armed: bool,
}

impl State {
    pub fn new(providers: Vec<(&'static str, &'static str)>, home: std::path::PathBuf) -> Self {
        Self {
            providers,
            home,
            items: Vec::new(),
            index: HashMap::new(),
            rows: Vec::new(),
            groups: Vec::new(),
            dirty: false,
            cursor: 0,
            moved: false,
            table: TableState::default(),
            rows_area: Rect::default(),
            sidebar_area: Rect::default(),
            page: 10,
            selected: HashSet::new(),
            sort: SortKey::Size,
            reverse: false,
            query: String::new(),
            provider: None,
            focus: Focus::Table,
            mode: Mode::Normal,
            detail: false,
            view: View::Flat,
            scanning: true,
            deleting: false,
            interrupted: 0,
            remeasuring: false,
            polls: 0,
            errors: Vec::new(),
            toast: None,
            spinner: 0,
            force_quit_armed: false,
        }
    }

    /// Forget everything found so far, for a rescan.
    pub fn reset_scan(&mut self) {
        self.items.clear();
        self.index.clear();
        self.rows.clear();
        self.selected.clear();
        self.errors.clear();
        self.reset_cursor();
        self.scanning = true;
    }

    /// Needs periodic redraws (spinners).
    pub fn busy(&self) -> bool {
        self.scanning || self.deleting
    }

    // ---- items ----

    /// Every item, visible or not.
    pub fn all_items(&self) -> impl Iterator<Item = &Item> {
        self.items.iter()
    }

    pub fn item(&self, id: ArtifactId) -> Option<&Item> {
        self.index.get(&id).map(|&i| &self.items[i])
    }

    pub fn push(&mut self, artifact: Artifact) {
        self.index.insert(artifact.id, self.items.len());
        self.items.push(Item {
            artifact,
            status: Status::Idle,
        });
        self.dirty = true;
    }

    pub fn measured(&mut self, id: ArtifactId, m: Measurement) {
        if let Some(&i) = self.index.get(&id) {
            let a = &mut self.items[i].artifact;
            a.size = Some(m.size);
            a.newest_mtime = m.newest_mtime;
            self.dirty = true;
        }
    }

    pub fn set_status(&mut self, id: ArtifactId, status: Status) {
        if let Some(&i) = self.index.get(&id) {
            self.items[i].status = status;
        }
    }

    pub fn remove(&mut self, id: ArtifactId) {
        let Some(i) = self.index.remove(&id) else {
            return;
        };
        self.items.remove(i);
        for (j, item) in self.items.iter().enumerate().skip(i) {
            self.index.insert(item.artifact.id, j);
        }
        // Keep `rows` valid right away: it indexes into `items`.
        self.rows.retain(|&r| r != i);
        for r in &mut self.rows {
            if *r > i {
                *r -= 1;
            }
        }
        self.cursor = self.cursor.min(self.visible_len().saturating_sub(1));
        self.selected.remove(&id);
        self.dirty = true;
    }

    // ---- view ----

    /// Visible items in display order.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &Item> {
        self.rows.iter().map(|&i| &self.items[i])
    }

    /// The item under the cursor (flat view only).
    pub fn current(&self) -> Option<&Item> {
        match self.view {
            View::Flat => self.rows.get(self.cursor).map(|&i| &self.items[i]),
            View::Projects => None,
        }
    }

    pub fn groups(&self) -> &[Group] {
        &self.groups
    }

    /// The group under the cursor (projects view only).
    pub fn current_group(&self) -> Option<&Group> {
        match self.view {
            View::Flat => None,
            View::Projects => self.groups.get(self.cursor),
        }
    }

    /// Number of rows in the current view.
    pub fn visible_len(&self) -> usize {
        match self.view {
            View::Flat => self.rows.len(),
            View::Projects => self.groups.len(),
        }
    }

    pub fn provider_filter(&self) -> Option<&'static str> {
        self.provider
    }

    /// Sidebar rows: "All" (`None`) first, then each provider that found something, biggest
    /// first. With totals in bytes.
    pub fn sidebar_entries(&self) -> Vec<(Option<&'static str>, u64)> {
        let mut totals: Vec<(&'static str, u64)> = Vec::new();
        for item in &self.items {
            let a = &item.artifact;
            match totals.iter_mut().find(|(id, _)| *id == a.provider) {
                Some((_, total)) => *total += a.on_disk(),
                None => totals.push((a.provider, a.on_disk())),
            }
        }
        // Stable sort: ties keep registry order.
        totals.sort_by_key(|&(id, total)| {
            let order = self.providers.iter().position(|(p, _)| *p == id);
            (Reverse(total), order)
        });
        let all = totals.iter().map(|(_, t)| t).sum();
        std::iter::once((None, all))
            .chain(totals.into_iter().map(|(id, t)| (Some(id), t)))
            .collect()
    }

    /// Back to the top row, e.g. after the sort or filter changed.
    pub fn reset_cursor(&mut self) {
        self.cursor = 0;
        self.moved = false;
    }

    /// The user moved the cursor here.
    pub fn move_cursor(&mut self, to: usize) {
        self.cursor = to.min(self.visible_len().saturating_sub(1));
        self.moved = true;
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Rebuild the visible rows if anything changed, keeping the cursor on the same item.
    pub fn refresh(&mut self) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        // The picked provider has nothing left (all deleted): back to "All".
        if let Some(p) = self.provider
            && !self.items.iter().any(|it| it.artifact.provider == p)
        {
            self.provider = None;
        }
        let current_item = self.current().map(|it| it.artifact.id);
        let current_group = self.current_group().map(|g| g.key.clone());

        let provider = self.provider_filter();
        let query = self.query.to_lowercase();
        let mut rows: Vec<usize> = (0..self.items.len())
            .filter(|&i| {
                let a = &self.items[i].artifact;
                provider.is_none_or(|p| a.provider == p)
                    && (query.is_empty() || self.haystack(a).contains(&query))
            })
            .collect();
        let items = &self.items;
        let sort = self.sort;
        rows.sort_by(|&x, &y| compare(sort, &items[x].artifact, &items[y].artifact));
        if self.reverse {
            rows.reverse();
        }
        self.rows = rows;
        self.groups = self.build_groups();

        let position = match self.view {
            View::Flat => current_item.and_then(|id| {
                self.rows
                    .iter()
                    .position(|&i| self.items[i].artifact.id == id)
            }),
            View::Projects => {
                current_group.and_then(|key| self.groups.iter().position(|g| g.key == key))
            }
        };
        let cursor = if self.moved {
            position.unwrap_or(self.cursor)
        } else {
            0
        };
        self.cursor = cursor.min(self.visible_len().saturating_sub(1));
    }

    /// Group the visible rows by project, keeping the row order within each group.
    fn build_groups(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = Vec::new();
        let mut index: HashMap<GroupKey, usize> = HashMap::new();
        for item in self.rows() {
            let a = &item.artifact;
            let key = match a.project() {
                Some(root) => GroupKey::Project(root.to_path_buf()),
                None => GroupKey::Global(a.provider),
            };
            let g = *index.entry(key.clone()).or_insert_with(|| {
                groups.push(self.new_group(key));
                groups.len() - 1
            });
            let group = &mut groups[g];
            group.members.push(a.id);
            if !group.kinds.contains(&a.label) {
                group.kinds.push(a.label.clone());
            }
            group.total += a.on_disk();
            group.newest = group.newest.max(a.newest_mtime);
            group.orphaned |= a.orphaned;
        }
        match self.sort {
            SortKey::Name => groups.sort_by(|a, b| a.name.cmp(&b.name)),
            _ => groups.sort_by_key(|g| Reverse(g.total)),
        }
        if self.reverse {
            groups.reverse();
        }
        groups
    }

    fn new_group(&self, key: GroupKey) -> Group {
        let name = match &key {
            GroupKey::Project(root) => human::path(root, &self.home),
            GroupKey::Global(provider) => format!("{provider} caches"),
        };
        Group {
            key,
            name,
            members: Vec::new(),
            kinds: Vec::new(),
            total: 0,
            newest: None,
            orphaned: false,
        }
    }

    fn haystack(&self, a: &Artifact) -> String {
        let location = human::location(a, &self.home);
        let place = human::place(a, &self.home);
        format!("{} {} {location} {place}", a.provider, a.label).to_lowercase()
    }

    // ---- selection & totals ----

    /// Can this item be selected for deletion right now?
    pub fn selectable(item: &Item) -> bool {
        item.status != Status::Deleting
    }

    pub fn selected_total(&self) -> u64 {
        self.selected
            .iter()
            .filter_map(|id| self.item(*id))
            .map(|it| it.artifact.on_disk())
            .sum()
    }

    pub fn is_safe_to_bulk_select(item: &Item) -> bool {
        Self::selectable(item) && item.artifact.safety != Safety::Caution
    }
}

fn compare(key: SortKey, a: &Artifact, b: &Artifact) -> Ordering {
    match key {
        SortKey::Size => Reverse(a.on_disk()).cmp(&Reverse(b.on_disk())),
        // Oldest first; unmeasured last.
        SortKey::Age => match (a.newest_mtime, b.newest_mtime) {
            (Some(x), Some(y)) => x.cmp(&y),
            (x, y) => x.is_none().cmp(&y.is_none()),
        },
        SortKey::Name => a.paths().cmp(b.paths()),
        SortKey::Provider => a
            .provider
            .cmp(b.provider)
            .then_with(|| Reverse(a.on_disk()).cmp(&Reverse(b.on_disk()))),
    }
}
