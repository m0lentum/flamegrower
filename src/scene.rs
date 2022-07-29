use starframe::{
    graph::{Graph, LayerViewMut, NodeRefMut},
    graphics::Mesh,
    math as m,
    physics::{Body, Collider, Material, Physics},
};

use assets_manager::{loader, Asset};

use crate::{
    fire::{Flammable, FlammableParams},
    player::PlayerSpawnPoint,
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
    //
    // world geometry
    //
    StaticCapsuleChain {
        pose: TiledPose,
        polyline: Vec<m::Vec2>,
        thickness: f64,
    },
    StaticCollider {
        pose: TiledPose,
        #[serde(flatten)]
        collider: TiledCollider,
    },
    //
    // interactive stuff
    //
    PlayerSpawnPoint {
        pose: TiledPose,
    },
    PhysicsObject {
        pose: TiledPose,
        #[serde(flatten)]
        collider: TiledCollider,
    },
    Weed {
        pose: TiledPose,
        #[serde(flatten)]
        collider: TiledCollider,
        #[serde(default = "true_")]
        is_static: bool,
    },
    Flamevine {
        pose: TiledPose,
        #[serde(flatten)]
        collider: TiledCollider,
        #[serde(default = "true_")]
        is_static: bool,
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
        ): &mut (
            LayerViewMut<m::Pose>,
            LayerViewMut<Collider>,
            LayerViewMut<Body>,
            LayerViewMut<Mesh>,
            LayerViewMut<Flammable>,
            LayerViewMut<PlayerSpawnPoint>,
        ),
    ) {
        match self {
            //
            // world geometry
            //
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
            Recipe::StaticCollider { pose, collider } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = collider.spawn(l_coll);
                let color = [1.0; 4];
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color(color));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
            }
            //
            // interactive stuff
            //
            Recipe::PlayerSpawnPoint { pose } => {
                let mut pose = l_pose.insert(pose.0);
                let mut marker = l_spawnpt.insert(PlayerSpawnPoint);
                pose.connect(&mut marker);
            }
            Recipe::PhysicsObject { pose, collider } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = collider.spawn(l_coll);
                let mut body = l_body.insert(Body::new_dynamic(coll.c.info(), 1.0));
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color([0.2, 0.6, 0.9, 1.0]));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                pose.connect(&mut body);
                body.connect(&mut coll);
            }
            Recipe::Weed {
                pose,
                collider,
                is_static,
            } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = collider.spawn(l_coll);
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color([0.2, 0.1, 0.5, 1.0]));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                if !is_static {
                    let mut body = l_body.insert(Body::new_dynamic(coll.c.info(), 1.0));
                    body.connect(&mut coll);
                    pose.connect(&mut body);
                }
                let mut flammable = l_flammable.insert(Flammable::new(FlammableParams {
                    time_to_destroy: Some(0.5),
                    ..Default::default()
                }));
                flammable.connect(&mut coll);
            }
            Recipe::Flamevine {
                pose,
                collider,
                is_static,
            } => {
                let mut pose = l_pose.insert(pose.0);
                let mut coll = collider.spawn(l_coll);
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color([0.9, 0.3, 0.0, 1.0]));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                if !is_static {
                    let mut body = l_body.insert(Body::new_dynamic(coll.c.info(), 1.0));
                    body.connect(&mut coll);
                    pose.connect(&mut body);
                }
                let eternal_fire = Flammable::new(FlammableParams {
                    time_to_destroy: None,
                    ..Default::default()
                })
                .ignited();
                let mut flammable = l_flammable.insert(eternal_fire);
                flammable.connect(&mut coll);
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

/// Non-polygon shapes produced by Tiled.
/// Symmetric shapes are sized based on width.
///
/// Use with `#[serde(flatten)]` in recipes.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct TiledCollider {
    width: f64,
    height: f64,
    #[serde(default)]
    shape: TiledColliderShape,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub enum TiledColliderShape {
    Circle,
    Rect,
    Capsule,
    Hexagon,
    Triangle,
}

impl Default for TiledColliderShape {
    fn default() -> Self {
        Self::Rect
    }
}

impl TiledColliderShape {
    pub fn realise(&self, width: f64, height: f64) -> Collider {
        match self {
            Self::Circle => Collider::new_circle(width),
            Self::Rect => Collider::new_rect(width, height),
            Self::Capsule => Collider::new_capsule(width, height / 2.0),
            Self::Hexagon => Collider::new_hexagon(width / 2.0),
            Self::Triangle => Collider::new_triangle(width / 2.0),
        }
    }
}

impl TiledCollider {
    pub fn spawn<'r, 'v: 'r>(
        &self,
        l_collider: &'r mut LayerViewMut<'v, Collider>,
    ) -> NodeRefMut<'r, Collider> {
        l_collider.insert(
            self.shape
                .realise(self.width, self.height)
                .with_material(DEFAULT_PHYSICS_MATERIAL),
        )
    }
}

/// Defaults for bool fields that aren't present
#[inline]
fn false_() -> bool {
    false
}

#[inline]
fn true_() -> bool {
    true
}
