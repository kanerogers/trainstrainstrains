use std::fmt::Display;

use common::hecs;

use crate::Resource;

#[derive(Debug, Clone)]
pub struct Job {
    pub place_of_work: hecs::Entity,
    pub state: JobState,
}

impl Job {
    pub fn new(place_of_work: hecs::Entity) -> Self {
        Self {
            place_of_work,
            state: JobState::GoingToPlaceOfWork,
        }
    }
}

#[derive(Debug, Clone)]
pub enum JobState {
    GoingToPlaceOfWork,
    Working(f32),
    DroppingOffResource(Resource, hecs::Entity),
    FetchingResource(Resource, hecs::Entity),
    Constructing,
}

impl Display for Job {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.state {
            JobState::GoingToPlaceOfWork => f.write_str("Going to place of work"),
            JobState::Working(a) => f.write_fmt(format_args!("Working - {a:.2}s")),
            JobState::DroppingOffResource(r, _) => {
                f.write_fmt(format_args!("Dropping off resource {r:?}"))
            }
            JobState::FetchingResource(r, _) => {
                f.write_fmt(format_args!("Fetching resource {r:?}"))
            }
            JobState::Constructing => f.write_str("Constructing something"),
        }
    }
}
