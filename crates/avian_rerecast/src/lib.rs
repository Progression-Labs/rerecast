//! Backend for using [`avian3d`](https://docs.rs/avian3d) with [`bevy_rerecast`](https://docs.rs/bevy_rerecast).

use avian3d::prelude::*;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use bevy_reflect::prelude::*;
use bevy_rerecast_core::{NavmeshApp as _, NavmeshSettings, ObstacleGather, rerecast::TriMesh};

mod collider_to_trimesh;
pub use crate::collider_to_trimesh::ColliderToTriMesh;
use crate::collider_to_trimesh::shape_to_trimesh_clipped;

/// Everything you need to get started with the Navmesh plugin.
pub mod prelude {
    pub use crate::{AvianBackendPlugin, ExcludeColliderFromNavmesh, NavmeshHeightfieldStride};
}

/// The plugin of the crate. Will make all entities with [`Collider`] a collider belonging to a static [`RigidBody`] available for navmesh generation.
#[non_exhaustive]
#[derive(Debug, Default)]
pub struct AvianBackendPlugin;

impl Plugin for AvianBackendPlugin {
    fn build(&self, app: &mut App) {
        app.set_navmesh_backend(collider_backend);
    }
}

/// Component to opt-out a [`Collider`] or [`RigidBody`] from navmesh generation when using [`AvianBackendPlugin`].
/// If that backend is not used, this component has no effect.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct ExcludeColliderFromNavmesh;

/// Feed a heightfield [`Collider`] to navmesh generation at every `n`-th row and column only.
/// For terrain sampled finer than the navmesh needs: the backend's cost (on the main thread)
/// and the rasterization both scale with the triangle count.
#[derive(Debug, Clone, Copy, Component, Reflect)]
#[reflect(Component)]
pub struct NavmeshHeightfieldStride(pub u32);

/// Picks the static colliders to bake, in the frame: their shared shapes (a reference count
/// each, no geometry copied) and poses. Converting them to triangles is the [`ObstacleGather`]'s,
/// which the generator runs in its task.
fn collider_backend(
    input: In<NavmeshSettings>,
    colliders: Query<
        (
            Entity,
            &Collider,
            &Position,
            &Rotation,
            &ColliderOf,
            Option<&NavmeshHeightfieldStride>,
        ),
        Without<ExcludeColliderFromNavmesh>,
    >,
    bodies: Query<&RigidBody, Without<ExcludeColliderFromNavmesh>>,
) -> ObstacleGather {
    let obstacles: Vec<_> = colliders
        .iter()
        .filter_map(|(entity, collider, pos, rot, collider_of, stride)| {
            if input
                .filter
                .as_ref()
                .is_some_and(|entities| !entities.contains(&entity))
            {
                return None;
            }
            let body = bodies.get(collider_of.body).ok()?;
            if !body.is_static() {
                return None;
            }
            let stride = stride.map_or(1, |s| s.0.max(1) as usize);
            Some((collider.shape_scaled().clone(), *pos, *rot, stride))
        })
        .collect();
    let clip = input.aabb;
    ObstacleGather::new(move || {
        let subdivisions = 10;
        obstacles
            .into_iter()
            .filter_map(|(shape, pos, rot, stride)| {
                shape_to_trimesh_clipped(&shape, pos, rot, subdivisions, clip.as_ref(), stride)
            })
            .fold(TriMesh::default(), |mut acc, t| {
                acc.extend(t);
                acc
            })
    })
}
