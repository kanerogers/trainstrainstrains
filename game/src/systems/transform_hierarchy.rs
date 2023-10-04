use components::{Parent, Transform};

use crate::Game;

// Literally copy-pasted from the `hecs` transform hierarchy example:
// https://github.com/Ralith/hecs/blob/master/examples/transform_hierarchy.rs
pub fn transform_hierarchy_system(game: &mut Game) {
    let world = &game.world;

    // Construct a view for efficient random access into the set of all entities that have
    // parents. Views allow work like dynamic borrow checking or component storage look-up to be
    // done once rather than per-entity as in `World::get`.
    let mut parents = world.query::<&Parent>();
    let parents = parents.view();

    // View of entities that don't have parents, i.e. roots of the transform hierarchy
    let mut roots = world.query::<&Transform>().without::<&Parent>();
    let roots = roots.view();

    // This query can coexist with the `roots` view without illegal aliasing of `Transform`
    // references because the inclusion of `&Parent` in the query, and its exclusion from the view,
    // guarantees that they will never overlap. Similarly, it can coexist with `parents` because
    // that view does not reference `Transform`s at all.
    for (_entity, (parent, absolute)) in world.query::<(&Parent, &mut Transform)>().iter() {
        // Walk the hierarchy from this entity to the root, accumulating the entity's absolute
        // transform. This does a small amount of redundant work for intermediate levels of deeper
        // hierarchies, but unlike a top-down traversal, avoids tracking entity child lists and is
        // cache-friendly.
        let mut relative = parent.offset;
        let mut ancestor = parent.entity;
        while let Some(next) = parents.get(ancestor) {
            relative = next.offset * relative;
            ancestor = next.entity;
        }
        // The `while` loop terminates when `ancestor` cannot be found in `parents`, i.e. when it
        // does not have a `Parent` component, and is therefore necessarily a root.
        *absolute = *roots.get(ancestor).unwrap() * relative;
    }
}
