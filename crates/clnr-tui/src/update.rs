//! State transitions. [`update`] is pure: anything touching the filesystem or threads is
//! returned as an [`Effect`] for the runtime to carry out.

use clnr_core::clean::{CleanEvent, Resumed, Summary};
use clnr_core::scan::ScanEvent;
use std::time::{Duration, SystemTime};

use clnr_core::{ArtifactId, Safety, human};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Position;

use crate::state::{Confirm, Focus, Mode, State, Status, View};

pub enum Msg {
    Input(Event),
    Scan(ScanEvent),
    /// Guard verdicts for a requested delete.
    Planned(PlanOutcome),
    Clean(CleanEvent),
    CleanDone(Summary),
    /// Deletes interrupted in an earlier run were finished.
    Resumed(Resumed),
    /// Time to consider re-measuring.
    Poll(SystemTime),
    /// A re-measured artifact's paths are all gone.
    Vanished(ArtifactId),
    RemeasureDone,
    Tick,
}

pub struct PlanOutcome {
    pub allowed: Vec<ArtifactId>,
    pub blocked: Vec<(ArtifactId, String)>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Effect {
    Quit,
    Rescan,
    /// Run the guard over these and answer with [`Msg::Planned`].
    Plan(Vec<ArtifactId>),
    /// Delete these (already confirmed) and stream [`Msg::Clean`] events back.
    Clean(Vec<ArtifactId>),
    /// Measure these again; answers with measurements, [`Msg::Vanished`], and
    /// [`Msg::RemeasureDone`].
    Remeasure(Vec<ArtifactId>),
}

/// Artifacts changed this recently count as active and are re-measured on every poll, so a
/// running build visibly grows.
const ACTIVE_WITHIN: Duration = Duration::from_secs(10 * 60);
/// Every this many polls, everything is re-measured, so a build starting in a quiet directory
/// is noticed too.
const FULL_EVERY: u64 = 12;

pub fn update(state: &mut State, msg: Msg) -> Vec<Effect> {
    match msg {
        Msg::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => on_key(state, key),
        Msg::Input(Event::Mouse(mouse)) => {
            on_mouse(state, mouse);
            vec![]
        }
        Msg::Input(_) => vec![],
        Msg::Scan(event) => {
            on_scan(state, event);
            vec![]
        }
        Msg::Planned(plan) => {
            on_planned(state, plan);
            vec![]
        }
        Msg::Clean(event) => {
            on_clean(state, event);
            vec![]
        }
        Msg::CleanDone(summary) => {
            state.deleting = false;
            state.force_quit_armed = false;
            let mut toast = format!(
                "Freed {} from {}.",
                human::size(summary.freed),
                human::count(summary.removed, "item")
            );
            if summary.failed + summary.blocked > 0 {
                toast += &format!(
                    " {} failed, {} skipped (press e).",
                    summary.failed, summary.blocked
                );
            }
            state.toast = Some(toast);
            vec![]
        }
        Msg::Resumed(resumed) => {
            state.interrupted = 0;
            state.errors.extend(resumed.errors);
            vec![]
        }
        Msg::Poll(now) => remeasure(state, now),
        Msg::Vanished(id) => {
            state.remove(id);
            vec![]
        }
        Msg::RemeasureDone => {
            state.remeasuring = false;
            vec![]
        }
        Msg::Tick => {
            state.spinner = state.spinner.wrapping_add(1);
            vec![]
        }
    }
}

/// Pick what to re-measure: active artifacts every poll, everything every [`FULL_EVERY`]th.
/// Skipped while scanning, deleting, or still measuring the previous batch.
fn remeasure(state: &mut State, now: SystemTime) -> Vec<Effect> {
    if state.scanning || state.deleting || state.remeasuring {
        return vec![];
    }
    let full = state.polls.is_multiple_of(FULL_EVERY);
    state.polls += 1;
    let active = |t: Option<SystemTime>| {
        t.is_some_and(|t| now.duration_since(t).is_ok_and(|age| age <= ACTIVE_WITHIN))
    };
    let ids: Vec<ArtifactId> = state
        .all_items()
        .filter(|it| it.status == Status::Idle && !it.artifact.paths().is_empty())
        .filter(|it| full || active(it.artifact.newest_mtime))
        .map(|it| it.artifact.id)
        .collect();
    if ids.is_empty() {
        return vec![];
    }
    state.remeasuring = true;
    vec![Effect::Remeasure(ids)]
}

fn on_scan(state: &mut State, event: ScanEvent) {
    match event {
        ScanEvent::Found(a) => state.push(*a),
        ScanEvent::Measured { id, measurement } => state.measured(id, measurement),
        ScanEvent::Error(e) => state.errors.push(e),
        ScanEvent::Done => state.scanning = false,
    }
}

fn on_planned(state: &mut State, plan: PlanOutcome) {
    if plan.allowed.is_empty() && plan.blocked.is_empty() {
        return;
    }
    let allowed_items = plan.allowed.iter().filter_map(|id| state.item(*id));
    let (total, needs_yes) = allowed_items.fold((0, false), |(total, caution), it| {
        (
            total + it.artifact.on_disk(),
            caution || it.artifact.safety == Safety::Caution,
        )
    });
    state.mode = Mode::Confirm(Confirm {
        allowed: plan.allowed,
        blocked: plan.blocked,
        total,
        needs_yes,
        typed: String::new(),
    });
}

fn on_clean(state: &mut State, event: CleanEvent) {
    match event {
        CleanEvent::Removed { id, .. } => state.remove(id),
        CleanEvent::Failed { id, error } => {
            state.set_status(id, Status::Failed);
            state.errors.push(error);
        }
        CleanEvent::Blocked { id, reason } => {
            state.set_status(id, Status::Idle);
            let path = state
                .item(id)
                .map(|it| human::describe(&it.artifact, &state.home))
                .unwrap_or_default();
            state.errors.push(format!("skipped {path}: {reason}"));
        }
    }
}

// ---- keys ----

fn on_key(state: &mut State, key: KeyEvent) -> Vec<Effect> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    if ctrl && key.code == KeyCode::Char('c') {
        return request_quit(state, true);
    }
    state.toast = None;
    match state.mode {
        Mode::Normal => normal_key(state, key),
        Mode::Filter => {
            filter_key(state, key);
            vec![]
        }
        Mode::Confirm(_) => confirm_key(state, key),
        Mode::Help | Mode::Errors => {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q' | '?' | 'e')
            ) {
                state.mode = Mode::Normal;
            }
            vec![]
        }
    }
}

/// Wheel scrolls the table; a click moves the cursor (or picks a provider), and a click in the
/// first column toggles selection. Only in normal mode: popups and prompts ignore the mouse.
fn on_mouse(state: &mut State, mouse: MouseEvent) {
    if !matches!(state.mode, Mode::Normal) {
        return;
    }
    let at = Position::new(mouse.column, mouse.row);
    match mouse.kind {
        MouseEventKind::ScrollDown => {
            state.focus = Focus::Table;
            move_by(state, 1);
        }
        MouseEventKind::ScrollUp => {
            state.focus = Focus::Table;
            move_by(state, -1);
        }
        MouseEventKind::Down(MouseButton::Left) if state.rows_area.contains(at) => {
            let row = state.table.offset() + usize::from(at.y - state.rows_area.y);
            if row >= state.visible_len() {
                return;
            }
            state.focus = Focus::Table;
            state.move_cursor(row);
            if at.x == state.rows_area.x {
                toggle_current(state);
            }
        }
        MouseEventKind::Down(MouseButton::Left) if state.sidebar_area.contains(at) => {
            // Inside the border; row 0 is "All".
            let row = usize::from(at.y.saturating_sub(state.sidebar_area.y + 1));
            if let Some(&(provider, _)) = state.sidebar_entries().get(row) {
                state.focus = Focus::Sidebar;
                pick_provider(state, provider);
            }
        }
        _ => {}
    }
}

fn request_quit(state: &mut State, forced: bool) -> Vec<Effect> {
    if !state.deleting || (forced && state.force_quit_armed) {
        return vec![Effect::Quit];
    }
    if forced {
        state.force_quit_armed = true;
        state.toast = Some("Deletes are running. Press ctrl-c again to quit anyway.".into());
    } else {
        state.toast =
            Some("Deletes are running; wait for them to finish (or ctrl-c twice).".into());
    }
    vec![]
}

fn normal_key(state: &mut State, key: KeyEvent) -> Vec<Effect> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    match key.code {
        KeyCode::Char('q') => return request_quit(state, false),
        KeyCode::Esc if !state.query.is_empty() => {
            state.query.clear();
            state.invalidate();
        }
        KeyCode::Esc if state.detail => state.detail = false,
        KeyCode::Esc => return request_quit(state, false),

        KeyCode::Tab | KeyCode::BackTab => {
            state.focus = match state.focus {
                Focus::Sidebar => Focus::Table,
                Focus::Table => Focus::Sidebar,
            };
        }
        KeyCode::Down | KeyCode::Char('j') => move_by(state, 1),
        KeyCode::Up | KeyCode::Char('k') => move_by(state, -1),
        KeyCode::PageDown => move_by(state, page(state)),
        KeyCode::PageUp => move_by(state, -page(state)),
        KeyCode::Char('d') if ctrl => move_by(state, page(state) / 2),
        KeyCode::Char('u') if ctrl => move_by(state, -page(state) / 2),
        KeyCode::Home | KeyCode::Char('g') => move_by(state, isize::MIN),
        KeyCode::End | KeyCode::Char('G') => move_by(state, isize::MAX),

        KeyCode::Char(' ') => toggle_current(state),
        KeyCode::Char('a') => {
            let ids: Vec<_> = state
                .rows()
                .filter(|it| State::is_safe_to_bulk_select(it))
                .map(|it| it.artifact.id)
                .collect();
            state.selected.extend(ids);
        }
        KeyCode::Char('A') => state.selected.clear(),

        KeyCode::Char('/') => state.mode = Mode::Filter,
        KeyCode::Char('s') => {
            state.sort = state.sort.next();
            state.invalidate();
        }
        KeyCode::Char('S') => {
            state.reverse = !state.reverse;
            state.invalidate();
        }
        KeyCode::Enter => state.detail = !state.detail,
        KeyCode::Char('v') => {
            state.view = match state.view {
                View::Flat => View::Projects,
                View::Projects => View::Flat,
            };
            state.reset_cursor();
            state.invalidate();
        }
        KeyCode::Char('e') => state.mode = Mode::Errors,
        KeyCode::Char('?') => state.mode = Mode::Help,

        KeyCode::Char('d') => return request_delete(state),
        KeyCode::Char('r') if state.deleting => {
            state.toast = Some("Can't rescan while deleting.".into());
        }
        KeyCode::Char('r') => return vec![Effect::Rescan],
        _ => {}
    }
    vec![]
}

fn page(state: &State) -> isize {
    state.page.max(1) as isize
}

fn move_by(state: &mut State, delta: isize) {
    match state.focus {
        Focus::Table => {
            let last = state.visible_len().saturating_sub(1);
            state.move_cursor(state.cursor.saturating_add_signed(delta).min(last));
        }
        Focus::Sidebar => {
            let entries = state.sidebar_entries();
            let current = entries
                .iter()
                .position(|(p, _)| *p == state.provider)
                .unwrap_or(0);
            let next = current
                .saturating_add_signed(delta)
                .min(entries.len().saturating_sub(1));
            pick_provider(state, entries[next].0);
        }
    }
}

fn pick_provider(state: &mut State, provider: Option<&'static str>) {
    if provider != state.provider {
        state.provider = provider;
        state.reset_cursor();
        state.invalidate();
    }
}

/// Space: toggle the current item, or all of the current project's non-caution items.
fn toggle_current(state: &mut State) {
    let ids = current_targets(state);
    if ids.is_empty() {
        return;
    }
    if ids.iter().all(|id| state.selected.contains(id)) {
        for id in &ids {
            state.selected.remove(id);
        }
    } else {
        state.selected.extend(ids);
    }
}

/// What the cursor points at, as deletable ids: the current item, or the current project's
/// non-caution items.
fn current_targets(state: &State) -> Vec<ArtifactId> {
    match state.view {
        View::Flat => state
            .current()
            .filter(|it| State::selectable(it))
            .map(|it| it.artifact.id)
            .into_iter()
            .collect(),
        View::Projects => state
            .current_group()
            .map(|g| {
                g.members
                    .iter()
                    .filter(|id| state.item(**id).is_some_and(State::is_safe_to_bulk_select))
                    .copied()
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn request_delete(state: &mut State) -> Vec<Effect> {
    if state.deleting {
        state.toast = Some("A delete is already running.".into());
        return vec![];
    }
    let ids: Vec<ArtifactId> = if state.selected.is_empty() {
        current_targets(state)
    } else {
        state.selected.iter().copied().collect()
    };
    let ids: Vec<ArtifactId> = ids
        .into_iter()
        .filter(|id| state.item(*id).is_some_and(State::selectable))
        .collect();
    if ids.is_empty() {
        vec![]
    } else {
        vec![Effect::Plan(ids)]
    }
}

fn filter_key(state: &mut State, key: KeyEvent) {
    match key.code {
        KeyCode::Enter => state.mode = Mode::Normal,
        KeyCode::Esc => {
            state.query.clear();
            state.mode = Mode::Normal;
        }
        KeyCode::Backspace => {
            state.query.pop();
        }
        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => state.query.push(c),
        _ => return,
    }
    state.reset_cursor();
    state.invalidate();
}

fn confirm_key(state: &mut State, key: KeyEvent) -> Vec<Effect> {
    let Mode::Confirm(confirm) = &mut state.mode else {
        return vec![];
    };
    let accepted = if confirm.needs_yes {
        match key.code {
            KeyCode::Char(c) => {
                confirm.typed.push(c);
                return vec![];
            }
            KeyCode::Backspace => {
                confirm.typed.pop();
                return vec![];
            }
            KeyCode::Enter => confirm.typed == "yes",
            KeyCode::Esc => false,
            _ => return vec![],
        }
    } else {
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => true,
            KeyCode::Char('n' | 'q') | KeyCode::Esc => false,
            _ => return vec![],
        }
    };

    let Mode::Confirm(confirm) = std::mem::replace(&mut state.mode, Mode::Normal) else {
        unreachable!("checked above");
    };
    if !accepted || confirm.allowed.is_empty() {
        return vec![];
    }
    for id in &confirm.allowed {
        state.set_status(*id, Status::Deleting);
        state.selected.remove(id);
    }
    state.deleting = true;
    vec![Effect::Clean(confirm.allowed)]
}
