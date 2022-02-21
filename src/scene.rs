use starframe::{
    graph::{Graph, LayerViewMut, NodeRefMut},
    graphics::Mesh,
    math as m,
    physics::{Body, Collider, Material, Physics},
};

use assets_manager::{loader, Asset};

use crate::{
    fire::{Flammable, FlammableParams},
    player::{Interactable, PlayerSpawnPoint},
};

/// A scene created with the Tiled editor.
///
/// Raw tiled scenes need to be run through `export.jq` to parse correctly.
/// See `export-scene` in `justfile`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Scene {
    recipes: Vec<Recipe>,
}
impl Asset for Scene {
    const EXTENSION: &'static str = "json";

    type Loader = loader::JsonLoader;
}

impl Scene {
    pub fn instantiate(&self, physics: &mut Physics, graph: &Graph) {
        let mut l = graph.get_layer_bundle();
        for recipe in self.recipes.iter() {
            recipe.spawn(physics, &mut l);
        }
    }
}

//
// concrete recipes
//

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(tag = "type")]
pub enum Recipe {
    PlayerSpawnPoint {
        pose: TiledPose,
    },
    StaticCapsuleChain {
        pose: TiledPose,
        polyline: Vec<m::Vec2>,
        thickness: f64,
    },
    StaticCollider {
        pose: TiledPose,
        #[serde(flatten)]
        collider: TiledSimpleShape,
        #[serde(default = "false_")]
        burn_target: bool,
    },
    PhysicsObject {
        pose: TiledPose,
        #[serde(flatten)]
        collider: TiledSimpleShape,
    },
    FireFlower {
        pose: TiledPose,
    },
}

impl Recipe {
    #[allow(clippy::type_complexity)]
    pub fn spawn(
        &self,
        _physics: &mut Physics, // will be used as soon as I get making nontrivial levels

        // more verbose than usual - taking layer bundle by reference here
        // to avoid some boilerplate in `Scene::instantiate`
        (
            ref mut l_pose,
            ref mut l_coll,
            ref mut l_body,
            ref mut l_mesh,
            ref mut l_flammable,
            ref mut l_spawnpt,
            ref mut l_interactable,
        ): &mut (
            LayerViewMut<m::Pose>,
            LayerViewMut<Collider>,
            LayerViewMut<Body>,
            LayerViewMut<Mesh>,
            LayerViewMut<Flammable>,
            LayerViewMut<PlayerSpawnPoint>,
            LayerViewMut<Interactable>,
        ),
    ) {
        match self {
            Recipe::PlayerSpawnPoint { pose } => {
                let mut pose = l_pose.insert(pose.0);
                let mut marker = l_spawnpt.insert(PlayerSpawnPoint);
                pose.connect(&mut marker);
            }
            Recipe::StaticCapsuleChain {
                pose,
                polyline,
                thickness,
            } => {
                let offset = pose.0.translation;
                let r = thickness / 2.0;
                for p in polyline.windows(2) {
                    let p: [m::Vec2; 2] = [offset + p[0], offset + p[1]];
                    let mid = (p[0] + p[1]) / 2.0;
                    let dist = p[1] - p[0];
                    let len = dist.mag();
                    let angle = f64::atan2(dist.y, dist.x);

                    let mut pose = l_pose.insert(m::Pose::new(mid, m::Angle::Rad(angle).into()));
                    let mut coll = l_coll.insert(Collider::new_capsule(len, r));
                    let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color([1.0; 4]));
                    pose.connect(&mut coll);
                    pose.connect(&mut mesh);
                }
            }
            Recipe::StaticCollider {
                pose,
                collider,
                burn_target,
            } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = collider.spawn(l_coll);
                let color = if *burn_target {
                    [1.0, 0.2, 0.3, 1.0]
                } else {
                    [1.0; 4]
                };
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color(color));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                if *burn_target {
                    let mut flammable = l_flammable.insert(Flammable::new(FlammableParams {
                        time_to_burn: 0.5,
                        ..Default::default()
                    }));
                    flammable.connect(&mut coll);
                }
            }
            Recipe::PhysicsObject { pose, collider } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = collider.spawn(l_coll);
                let mut body = l_body.insert(Body::new_dynamic(coll.c, 1.0));
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color([0.2, 0.6, 0.9, 1.0]));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                pose.connect(&mut body);
                body.connect(&mut coll);
            }
            Recipe::FireFlower { pose } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = l_coll.insert(
                    Collider::new_circle(0.5)
                        .trigger()
                        .with_layer(crate::collision_layers::INTERACTABLE),
                );
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color([0.9, 0.3, 0.0, 1.0]));
                let mut tag = l_interactable.insert(Interactable::FireFlower { taken: false });
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                coll.connect(&mut tag);
            }
        }
    }
}

//
// utility types for deserializing tiled
// and spawning common patterns
//

/// Pose deserialized from Tiled data. Every Tiled object has this.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
#[serde(from = "TiledPoseDeser")]
pub struct TiledPose(pub m::Pose);

#[derive(Clone, Copy, Debug, serde::Deserialize)]
struct TiledPoseDeser {
    x: f64,
    y: f64,
    rotation: f64,
}

impl From<TiledPoseDeser> for TiledPose {
    fn from(p: TiledPoseDeser) -> Self {
        Self(m::Pose::new(
            m::Vec2::new(p.x, p.y),
            m::Angle::Rad(p.rotation).into(),
        ))
    }
}

impl From<TiledPose> for m::Pose {
    fn from(p: TiledPose) -> Self {
        p.0
    }
}

/// Default physics material should allow player to push boxes
/// but also rotate large ones by grabbing a high corner and pulling down
const DEFAULT_PHYSICS_MATERIAL: Material = Material {
    // big TODO: fix static friction being stronger in one direction
    // probably best done with a block solver
    static_friction_coef: None,
    dynamic_friction_coef: Some(0.2),
    restitution_coef: 0.0,
};

/// Non-polygon shapes produced by Tiled (with capsule being a custom extension)
///
/// Needs to be used with `#[serde(flatten)]` in recipes
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct TiledSimpleShape {
    width: f64,
    height: f64,
    #[serde(default = "false_")]
    ellipse: bool,
    #[serde(default = "false_")]
    capsule: bool,
}

impl TiledSimpleShape {
    pub fn spawn<'r, 'v: 'r>(
        &self,
        l_collider: &'r mut LayerViewMut<'v, Collider>,
    ) -> NodeRefMut<'r, Collider> {
        l_collider.insert(
            if self.ellipse {
                Collider::new_circle(self.width / 2.0)
            } else if self.capsule {
                Collider::new_capsule(self.width, self.height)
            } else {
                Collider::new_rect(self.width, self.height)
            }
            .with_material(DEFAULT_PHYSICS_MATERIAL),
        )
    }
}

/// Default for bool fields that aren't present
#[inline]
fn false_() -> bool {
    false
}
