use starframe as sf;

use assets_manager::{loader, Asset};

use crate::{
    fire::{Flammable, FlammableParams},
    player::PlayerSpawnPoint,
};

/// Default physics material should allow player to push boxes
/// but also rotate large ones by grabbing a high corner and pulling down
const DEFAULT_PHYSICS_MATERIAL: sf::PhysicsMaterial = sf::PhysicsMaterial {
    // big TODO: fix static friction being stronger in one direction
    // probably best done with a block solver
    static_friction_coef: None,
    dynamic_friction_coef: Some(0.2),
    restitution_coef: 0.0,
};

const DEFAULT_BODY_DENSITY: f64 = 0.25;

/// A scene created with the Tiled editor.
///
/// Raw tiled scenes need to be run through `export.jq` to parse correctly.
/// See `export-scene` in `justfile`.
#[derive(Clone, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct Scene {
    // temporary hack to see the whole level before I have proper camera control
    initial_camera_zoom: f64,
    recipes: Vec<Recipe>,
}
impl Asset for Scene {
    const EXTENSION: &'static str = "json";

    type Loader = loader::JsonLoader;
}

impl Scene {
    pub fn instantiate(
        &self,
        camera: &mut sf::Camera,
        physics: &mut sf::Physics,
        graph: &sf::Graph,
    ) {
        camera.transform.scale = self.initial_camera_zoom;

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
        polyline: Vec<sf::Vec2>,
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

sf::named_layer_bundle! {
    pub struct SpawnLayers<'a> {
        pose: w sf::Pose,
        coll: w sf::Collider,
        body: w sf::Body,
        mesh: w sf::Mesh,
        flammable: w Flammable,
        spawnpt: w PlayerSpawnPoint,
    }
}

impl Recipe {
    pub fn spawn(
        &self,
        _physics: &mut sf::Physics, // will be used as soon as I get making nontrivial levels
        l: &mut SpawnLayers,
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
                    let p: [sf::Vec2; 2] = [offset + p[0], offset + p[1]];
                    let mid = (p[0] + p[1]) / 2.0;
                    let dist = p[1] - p[0];
                    let len = dist.mag();
                    let angle = f64::atan2(dist.y, dist.x);

                    let mut pose = l
                        .pose
                        .insert(sf::Pose::new(mid, sf::Angle::Rad(angle).into()));
                    let mut coll = l.coll.insert(sf::Collider::new_capsule(len, r));
                    let mut mesh = l.mesh.insert(sf::Mesh::from(*coll.c).with_color([1.0; 4]));
                    pose.connect(&mut coll);
                    pose.connect(&mut mesh);
                }
            }
            Recipe::StaticCollider { pose, collider } => {
                let mut pose = l.pose.insert(pose.0);
                let mut coll = collider.spawn(&mut l.coll);
                let color = [1.0; 4];
                let mut mesh = l.mesh.insert(sf::Mesh::from(*coll.c).with_color(color));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
            }
            //
            // interactive stuff
            //
            Recipe::PlayerSpawnPoint { pose } => {
                let mut pose = l.pose.insert(pose.0);
                let mut marker = l.spawnpt.insert(PlayerSpawnPoint);
                pose.connect(&mut marker);
            }
            Recipe::PhysicsObject { pose, collider } => {
                let mut pose = l.pose.insert(pose.0);
                let mut coll = collider.spawn(&mut l.coll);
                let mut body = l
                    .body
                    .insert(sf::Body::new_dynamic(coll.c.info(), DEFAULT_BODY_DENSITY));
                let mut mesh = l
                    .mesh
                    .insert(sf::Mesh::from(*coll.c).with_color([0.2, 0.6, 0.9, 1.0]));
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
                let mut pose = l.pose.insert(pose.0);
                let mut coll = collider.spawn(&mut l.coll);
                let mut mesh = l
                    .mesh
                    .insert(sf::Mesh::from(*coll.c).with_color([0.05, 0.02, 0.1, 1.0]));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                if !is_static {
                    let mut body = l
                        .body
                        .insert(sf::Body::new_dynamic(coll.c.info(), DEFAULT_BODY_DENSITY));
                    body.connect(&mut coll);
                    pose.connect(&mut body);
                }
                let mut flammable = l.flammable.insert(Flammable::new(FlammableParams {
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
                let mut pose = l.pose.insert(pose.0);
                let mut coll = collider.spawn(&mut l.coll);
                let mut mesh = l
                    .mesh
                    .insert(sf::Mesh::from(*coll.c).with_color([0.9, 0.3, 0.0, 1.0]));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                if !is_static {
                    let mut body = l.body.insert(sf::Body::new_dynamic(coll.c.info(), 1.0));
                    body.connect(&mut coll);
                    pose.connect(&mut body);
                }
                let eternal_fire = Flammable::new(FlammableParams {
                    time_to_destroy: None,
                    ..Default::default()
                })
                .ignited();
                let mut flammable = l.flammable.insert(eternal_fire);
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
pub struct TiledPose(pub sf::Pose);

#[derive(Clone, Copy, Debug, serde::Deserialize)]
struct TiledPoseDeser {
    x: f64,
    y: f64,
    rotation: f64,
}

impl From<TiledPoseDeser> for TiledPose {
    fn from(p: TiledPoseDeser) -> Self {
        Self(sf::Pose::new(
            sf::Vec2::new(p.x, p.y),
            sf::Angle::Rad(p.rotation).into(),
        ))
    }
}

impl From<TiledPose> for sf::Pose {
    fn from(p: TiledPose) -> Self {
        p.0
    }
}

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
    #[serde(default)]
    corner_radius: f64,
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
    pub fn realise(&self, width: f64, height: f64) -> sf::Collider {
        match self {
            Self::Circle => sf::Collider::new_circle(width / 2.0),
            Self::Rect => sf::Collider::new_rect(width, height),
            Self::Capsule => sf::Collider::new_capsule(width, height / 2.0),
            Self::Hexagon => sf::Collider::new_hexagon(width / 2.0),
            Self::Triangle => sf::Collider::new_triangle(width / 2.0),
        }
    }
}

impl TiledCollider {
    pub fn spawn<'r, 'v: 'r>(
        &self,
        l_collider: &'r mut sf::LayerViewMut<'v, sf::Collider>,
    ) -> sf::NodeRefMut<'r, sf::Collider> {
        let mut coll = self
            .shape
            .realise(self.width, self.height)
            .with_material(DEFAULT_PHYSICS_MATERIAL);
        if self.corner_radius > 0.0 {
            coll.shape = coll.shape.rounded_inward(self.corner_radius);
        }
        l_collider.insert(coll)
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
