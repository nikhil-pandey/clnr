//! The event loop: gathers messages from the terminal, the scan, and deletes, feeds them to
//! [`update`], and carries out the resulting [`Effect`]s.

use std::collections::VecDeque;
use std::io;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use clnr_core::clean::{self, Guard, Journal, Plan};
use clnr_core::scan::{Scan, ScanEvent, measure_paths};
use clnr_core::{Artifact, ArtifactId};
use crossbeam_channel::{Receiver, Select, Sender, after, never, tick, unbounded};
use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event};
use rayon::prelude::*;

use crate::Context;
use crate::state::State;
use crate::theme::Theme;
use crate::ui;
use crate::update::{Effect, Msg, PlanOutcome, update};

/// Redraw interval while something is in progress (spinners).
const TICK: Duration = Duration::from_millis(200);
/// How often to consider re-measuring; see [`crate::update`] for what gets measured.
const POLL: Duration = Duration::from_secs(5);

pub struct Runtime {
    ctx: Context,
    theme: Theme,
    state: State,
    scan: Scan,
    /// The current scan's events; [`never`] once it is finished.
    scan_rx: Receiver<ScanEvent>,
    input_rx: Receiver<Event>,
    poll_rx: Receiver<Instant>,
    /// Background work (deletes, re-measuring) reports here.
    bg_tx: Sender<Msg>,
    journal: Arc<Journal>,
    bg_rx: Receiver<Msg>,
    /// Messages produced by effects, handled before new input.
    pending: VecDeque<Msg>,
}

impl Runtime {
    pub fn new(ctx: Context) -> Self {
        let providers = ctx
            .registry
            .providers()
            .iter()
            .map(|p| (p.id(), p.name()))
            .collect();
        let mut state = State::new(providers, ctx.env.home().to_path_buf());
        let theme = Theme::new(ctx.env.var("NO_COLOR").is_some(), ctx.ascii);
        let scan = Scan::start(Arc::clone(&ctx.registry), &ctx.env, &ctx.options);
        let scan_rx = scan.events().clone();
        let (bg_tx, bg_rx) = unbounded();
        // Read-only here: leftovers are finished only along with a confirmed delete.
        let journal = Arc::new(Journal::in_state_dir(&ctx.env));
        state.interrupted = clean::resumable(&journal, &ctx.env, &ctx.options.protect).len();
        Self {
            ctx,
            theme,
            state,
            scan,
            scan_rx,
            input_rx: spawn_input_reader(),
            poll_rx: tick(POLL),
            bg_tx,
            journal,
            bg_rx,
            pending: VecDeque::new(),
        }
    }

    pub fn run(mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        loop {
            self.state.refresh();
            terminal.draw(|frame| ui::draw(frame, &mut self.state, &self.theme))?;

            // Block for one message, then drain whatever else is queued so a burst of scan
            // events costs one redraw.
            let mut batch: Vec<Msg> = self.pending.drain(..).collect();
            if batch.is_empty() {
                batch.extend(self.next(true));
            }
            while let Some(msg) = self.next(false) {
                batch.push(msg);
            }

            for msg in batch {
                for effect in update(&mut self.state, msg) {
                    if effect == Effect::Quit {
                        self.scan.cancel();
                        return Ok(());
                    }
                    self.apply(effect);
                }
            }
        }
    }

    /// The next message, waiting for one if `wait`.
    fn next(&mut self, wait: bool) -> Option<Msg> {
        let tick = if self.state.busy() {
            after(TICK)
        } else {
            never()
        };
        let mut sel = Select::new();
        let input = sel.recv(&self.input_rx);
        let scan = sel.recv(&self.scan_rx);
        let background = sel.recv(&self.bg_rx);
        let polled = sel.recv(&self.poll_rx);
        let ticked = sel.recv(&tick);
        let op = if wait {
            sel.select()
        } else {
            sel.try_select().ok()?
        };

        let index = op.index();
        let (msg, closed) = if index == input {
            split(op.recv(&self.input_rx).map(Msg::Input))
        } else if index == scan {
            split(op.recv(&self.scan_rx).map(Msg::Scan))
        } else if index == background {
            split(op.recv(&self.bg_rx))
        } else if index == polled {
            split(op.recv(&self.poll_rx).map(|_| Msg::Poll(SystemTime::now())))
        } else {
            debug_assert_eq!(index, ticked);
            split(op.recv(&tick).map(|_| Msg::Tick))
        };
        drop(sel);

        // A closed channel would be ready forever; stop selecting on it.
        if closed {
            if index == input {
                self.input_rx = never();
            } else if index == scan {
                self.scan_rx = never();
            }
        }
        msg
    }

    fn apply(&mut self, effect: Effect) {
        match effect {
            Effect::Quit => {}
            Effect::Rescan => {
                self.scan.cancel();
                self.state.reset_scan();
                self.scan = Scan::start(
                    Arc::clone(&self.ctx.registry),
                    &self.ctx.env,
                    &self.ctx.options,
                );
                self.scan_rx = self.scan.events().clone();
            }
            Effect::Plan(ids) => {
                let guard = Guard::new(&self.ctx.registry, self.scan.scope(), &self.ctx.env);
                let plan = Plan::new(&guard, self.artifacts(&ids));
                self.pending.push_back(Msg::Planned(PlanOutcome {
                    allowed: plan.allowed().map(|a| a.id).collect(),
                    blocked: plan.blocked().map(|(a, b)| (a.id, b.to_string())).collect(),
                }));
            }
            Effect::Clean(ids) => self.spawn_clean(self.artifacts(&ids)),
            Effect::Remeasure(ids) => self.spawn_remeasure(self.artifacts(&ids)),
        }
    }

    fn artifacts(&self, ids: &[ArtifactId]) -> Vec<Artifact> {
        ids.iter()
            .filter_map(|id| self.state.item(*id))
            .map(|it| it.artifact.clone())
            .collect()
    }

    /// Re-measure on a background thread. Artifacts whose paths are all gone are reported as
    /// vanished.
    fn spawn_remeasure(&self, artifacts: Vec<Artifact>) {
        let tx = self.bg_tx.clone();
        thread::spawn(move || {
            artifacts.par_iter().for_each(|a| {
                let msg = if a.paths().iter().any(|p| p.symlink_metadata().is_ok()) {
                    Msg::Scan(ScanEvent::Measured {
                        id: a.id,
                        measurement: measure_paths(a.paths()),
                    })
                } else {
                    Msg::Vanished(a.id)
                };
                let _ = tx.send(msg);
            });
            let _ = tx.send(Msg::RemeasureDone);
        });
    }

    /// Delete on a background thread; the guard re-checks every item right before it goes.
    fn spawn_clean(&self, artifacts: Vec<Artifact>) {
        let registry = Arc::clone(&self.ctx.registry);
        let env = self.ctx.env.clone();
        let scope = self.scan.scope().clone();
        let tx = self.bg_tx.clone();
        let journal = Arc::clone(&self.journal);
        let protect = self.ctx.options.protect.clone();
        thread::spawn(move || {
            // The user confirmed this delete, which included any interrupted earlier.
            let resumed = clean::resume(&journal, &env, &protect);
            if resumed.removed > 0 || !resumed.errors.is_empty() {
                let _ = tx.send(Msg::Resumed(resumed));
            }
            let guard = Guard::new(&registry, &scope, &env);
            let refs: Vec<&Artifact> = artifacts.iter().collect();
            let summary = clean::run(&guard, &journal, &refs, &|event| {
                let _ = tx.send(Msg::Clean(event));
            });
            let _ = tx.send(Msg::CleanDone(summary));
        });
    }
}

/// `(message, channel closed)`.
fn split<T, E>(result: Result<T, E>) -> (Option<T>, bool) {
    match result {
        Ok(msg) => (Some(msg), false),
        Err(_) => (None, true),
    }
}

/// Terminal events on a channel, so they can be selected alongside everything else.
fn spawn_input_reader() -> Receiver<Event> {
    let (tx, rx) = unbounded();
    thread::spawn(move || {
        while let Ok(event) = event::read() {
            if tx.send(event).is_err() {
                break;
            }
        }
    });
    rx
}
