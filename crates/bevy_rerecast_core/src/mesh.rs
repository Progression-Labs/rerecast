use alloc::vec::Vec;
use bevy_app::prelude::*;
use bevy_asset::prelude::*;
use bevy_ecs::prelude::*;
use bevy_mesh::{Mesh, Mesh3d, PrimitiveTopology};
use bevy_reflect::prelude::*;
use bevy_transform::components::GlobalTransform;
use glam::{UVec3, Vec3A};
use rerecast::{AreaType, TriMesh};

use crate::{NavmeshApp as _, NavmeshSettings, ObstacleGather};

/// A backend for navmesh generation.
/// Uses all entities with a [`Mesh3d`] component as navmesh obstacles.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Mesh3dBackendPlugin;

impl Plugin for Mesh3dBackendPlugin {
    fn build(&self, app: &mut App) {
        app.set_navmesh_backend(mesh3d_backend);
        app.register_type::<ExcludeMeshFromNavmesh>();
    }
}

/// Component to opt-out a [`Mesh3d`] from navmesh generation when using [`Mesh3dBackendPlugin`].
/// If that backend is not used, this component has no effect.
#[derive(Debug, Default, Component, Reflect)]
#[reflect(Component)]
pub struct ExcludeMeshFromNavmesh;

/// Picks the meshes in the frame. A mesh asset cannot be read from the task, so this copies
/// what the conversion reads, each mesh's positions and indices (not its other attributes), with
/// its transform; transforming them into triangles is the [`ObstacleGather`]'s, run in the
/// generator's task.
fn mesh3d_backend(
    input: In<NavmeshSettings>,
    meshes: Res<Assets<Mesh>>,
    obstacles: Query<(Entity, &GlobalTransform, &Mesh3d), Without<ExcludeMeshFromNavmesh>>,
) -> ObstacleGather {
    let obstacles: Vec<_> = obstacles
        .iter()
        .filter_map(|(entity, transform, mesh)| {
            if input
                .filter
                .as_ref()
                .is_some_and(|entities| !entities.contains(&entity))
            {
                return None;
            }
            Some((MeshTriangles::of(meshes.get(mesh)?)?, *transform))
        })
        .collect();
    ObstacleGather::new(move || {
        obstacles
            .into_iter()
            .map(|(mesh, transform)| mesh.into_trimesh(&transform))
            .fold(TriMesh::default(), |mut acc, t| {
                acc.extend(t);
                acc
            })
    })
}

/// What [`TriMeshFromBevyMesh::from_mesh`] reads of a mesh: its positions and triangle indices.
struct MeshTriangles {
    positions: Vec<[f32; 3]>,
    indices: Vec<UVec3>,
}

impl MeshTriangles {
    /// `None` for a mesh that is not a triangle list or has no float3 positions or indices.
    fn of(mesh: &Mesh) -> Option<Self> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return None;
        }
        let positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)?
            .as_float3()?
            .to_vec();
        let indices: Vec<_> = mesh.indices()?.iter().collect();
        if !indices.len().is_multiple_of(3) {
            return None;
        }
        let indices = indices
            .chunks(3)
            .map(|i| UVec3::new(i[0] as u32, i[1] as u32, i[2] as u32))
            .collect();
        Some(Self { positions, indices })
    }

    fn into_trimesh(self, transform: &GlobalTransform) -> TriMesh {
        let area_types = vec![AreaType::NOT_WALKABLE; self.indices.len()];
        TriMesh {
            vertices: self
                .positions
                .into_iter()
                .map(|p| transform.affine().transform_point3a(Vec3A::from_array(p)))
                .collect(),
            indices: self.indices,
            area_types,
        }
    }
}

/// Used to add [`TriMeshFromBevyMesh::from_mesh`] to [`TriMesh`].
pub trait TriMeshFromBevyMesh {
    /// Converts a [`Mesh`] into a [`TriMesh`].
    fn from_mesh(mesh: &Mesh) -> Option<TriMesh>;
}

impl TriMeshFromBevyMesh for TriMesh {
    fn from_mesh(mesh: &Mesh) -> Option<TriMesh> {
        Some(MeshTriangles::of(mesh)?.into_trimesh(&GlobalTransform::IDENTITY))
    }
}
