use starframe::{
    graph::{Graph, LayerViewMut},
    graphics::Mesh,
    math as m,
    physics::{Collider, ColliderShape, Physics},
};

use assets_manager::{loader, Asset};

use crate::{
    fire::{Flammable, FlammableParams},
    player::PlayerSpawnPoint,
};

/// A scene created with the Tiled editor.
///
/// Raw tiled scenes need to be run through `export.jq` to parse correctly.
/// See `just export-scene`.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default)]
pub struct Scene {
    recipes: Vec<Recipe>,
}
impl Default for Scene {
    fn default() -> Self {
        Self { recipes: vec![] }
    }
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
// recipes
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
        width: f64,
        height: f64,
        #[serde(default = "false_")]
        ellipse: bool,
        #[serde(default = "false_")]
        capsule: bool,
        #[serde(default = "false_")]
        burn_target: bool,
    },
}

impl Recipe {
    #[allow(clippy::type_complexity)]
    pub fn spawn(
        &self,
        _physics: &mut Physics, // will be used as soon as I get making nontrivial levels
        // taking layer bundle by reference here to avoid some boilerplate with subviews in
        // `Scene::instantiate`
        (ref mut l_pose, ref mut l_coll, ref mut l_mesh, ref mut l_flammable, ref mut l_spawnpt): &mut (
            LayerViewMut<m::Pose>,
            LayerViewMut<Collider>,
            LayerViewMut<Mesh>,
            LayerViewMut<Flammable>,
            LayerViewMut<PlayerSpawnPoint>,
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
                width,
                height,
                ellipse,
                capsule,
                burn_target,
            } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = l_coll.insert(Collider {
                    shape: if *ellipse {
                        ColliderShape::Circle { r: width / 2.0 }
                    } else if *capsule {
                        ColliderShape::Capsule {
                            hl: width / 2.0,
                            r: height / 2.0,
                        }
                    } else {
                        ColliderShape::Rect {
                            hw: width / 2.0,
                            hh: height / 2.0,
                        }
                    },
                    ..Default::default()
                });
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
        }
    }
}

//
// utility types for deserializing tiled
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

/// Default for bool fields that aren't present
#[inline]
fn false_() -> bool {
    false
}
