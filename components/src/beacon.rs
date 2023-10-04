use std::collections::HashSet;

use common::hecs;

#[derive(Debug, Clone, Default)]
pub struct Beacon {
    pub workers: HashSet<hecs::Entity>,
}
