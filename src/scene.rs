use starframe::{
    graph::{Graph, LayerViewMut},
    graphics::Mesh,
    math as m,
    physics::{Collider, Physics},
};

use assets_manager::{loader, Asset};

use crate::fire::{Flammable, FlammableParams};

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default)]
pub struct Scene {
    pub player_start: m::Vec2,
    recipes: Vec<Recipe>,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            player_start: m::Vec2::zero(),
            recipes: vec![],
        }
    }
}
impl Asset for Scene {
    const EXTENSION: &'static str = "ron";

    type Loader = loader::RonLoader;
}

impl Scene {
    pub fn instantiate(&self, physics: &mut Physics, graph: &Graph) {
        for recipe in &self.recipes {
            recipe.spawn(physics, graph.get_layer_bundle());
        }
    }
}

//
// recipes
//

#[derive(Clone, Debug, serde::Deserialize)]
pub enum Recipe {
    StaticCapsuleChain {
        width: f64,
        points: Vec<[f64; 2]>,
    },
    StaticCollider {
        pose: m::PoseBuilder,
        coll: Collider,
        is_burn_target: bool,
    },
}

impl Recipe {
    pub fn spawn(
        &self,
        _physics: &mut Physics, // will be used as soon as I get making nontrivial levels
        (mut l_pose, mut l_coll, mut l_mesh, mut l_flammable): (
            LayerViewMut<m::Pose>,
            LayerViewMut<Collider>,
            LayerViewMut<Mesh>,
            LayerViewMut<Flammable>,
        ),
    ) {
        match self {
            Recipe::StaticCapsuleChain { width, points } => {
                let r = width / 2.0;
                for p in points.windows(2) {
                    let p: [m::Vec2; 2] = [p[0].into(), p[1].into()];
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
                coll,
                is_burn_target,
            } => {
                let mut pose = l_pose.insert(pose.build());
                let mut coll = l_coll.insert(*coll);
                let color = if *is_burn_target {
                    [1.0, 0.2, 0.3, 1.0]
                } else {
                    [1.0; 4]
                };
                let mut mesh = l_mesh.insert(Mesh::from(*coll.c).with_color(color));
                pose.connect(&mut coll);
                pose.connect(&mut mesh);
                if *is_burn_target {
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
