//! The fleet work a host can run away from the frame: the web build has no threads, so it hands
//! these to a worker, which runs `Worker::serve` on its own copy of the data.

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use spai_fit::calc::Env;
use spai_fit::fit::Fit;
use spai_fit::fleet::{ammo::AmmoChoice, Fleet, FleetStats};
use spai_fit::skills::SkillSet;
use spai_fit::Dogma;

use crate::fleet_panel::{Analysis, FightJob};

#[derive(Serialize, Deserialize)]
pub enum WorkRequest {
    /// A fleet's numbers, with the fits its slots and boosters name and each slot's pilot.
    Stats { fleet: Fleet, fits: HashMap<String, Fit>, skills: Vec<(Option<String>, SkillSet)> },
    /// One fit's ammo options.
    Ammo { fit: Fit, skills: SkillSet, env: Env },
    /// The fight between two fleets whose numbers are in.
    Fight { fleet: Fleet, enemy: Fleet, mine: Arc<FleetStats>, theirs: Arc<FleetStats> },
}

#[derive(Serialize, Deserialize)]
pub enum WorkReply {
    /// How far a fight is, 0 to 1.
    Progress(f32),
    Stats(FleetStats),
    Ammo(Vec<AmmoChoice>),
    /// The fight, and the slots' ammo options it worked out.
    Fight(Analysis, Vec<(u64, Arc<Vec<AmmoChoice>>)>),
}

/// Somewhere the host runs a request off the frame. `reply` is called on the frame's thread with
/// each reply, the last one the result.
pub trait OffFrame {
    fn run(&self, request: WorkRequest, reply: Box<dyn FnMut(WorkReply)>);
    /// False once it cannot take work, when the frame works it out itself; the replies it still
    /// owes are dropped, which ends their runs.
    fn alive(&self) -> bool {
        true
    }
}

/// MessagePack with field names: the fits skip their empty fields, which a format without them
/// cannot read back.
pub fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    rmp_serde::to_vec_named(value).unwrap_or_default()
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> Option<T> {
    rmp_serde::from_slice(bytes).ok()
}

/// A fleet's numbers from fits and pilots at hand: what a thread or a worker runs.
pub fn stats(dogma: &Dogma, fleet: &Fleet, fits: &HashMap<String, Fit>, skills: &[(Option<String>, SkillSet)]) -> FleetStats {
    let fit = |id: &str| fits.get(id).cloned();
    let fallback = skills.iter().find(|(p, _)| p.is_none()).map(|(_, s)| s.clone()).unwrap_or_default();
    let skill = |p: Option<&str>| skills.iter().find(|(x, _)| x.as_deref() == p).map_or_else(|| fallback.clone(), |(_, s)| s.clone());
    spai_fit::fleet::stats(dogma, fleet, &fit, &skill)
}

/// The worker's side: the data, and the ammo options of the fights it has run.
pub struct Worker {
    dogma: Arc<Dogma>,
    ammo: Vec<(u64, Arc<Vec<AmmoChoice>>)>,
}

impl Worker {
    pub fn new(dogma: Arc<Dogma>) -> Self {
        Worker { dogma, ammo: Vec::new() }
    }

    pub fn serve(&mut self, request: WorkRequest, reply: &mut dyn FnMut(WorkReply)) {
        match request {
            WorkRequest::Stats { fleet, fits, skills } => reply(WorkReply::Stats(stats(&self.dogma, &fleet, &fits, &skills))),
            WorkRequest::Ammo { fit, skills, env } => reply(WorkReply::Ammo(spai_fit::fleet::ammo::choices(&self.dogma, &fit, &skills, &env))),
            WorkRequest::Fight { fleet, enemy, mine, theirs } => {
                let mut job = FightJob::new(self.dogma.clone(), fleet, enemy, mine, theirs, self.ammo.clone());
                let mut sent = 0.0f32;
                while !job.step() {
                    // A message a percent: each is copied across to the page.
                    let p = job.progress();
                    if p - sent >= 0.01 {
                        sent = p;
                        reply(WorkReply::Progress(p));
                    }
                }
                let (analysis, fresh) = job.finish();
                self.ammo.extend(fresh.iter().cloned());
                let over = self.ammo.len().saturating_sub(crate::fleet_panel::CACHED_SLOTS);
                self.ammo.drain(..over);
                reply(WorkReply::Fight(analysis, fresh));
            }
        }
    }
}
