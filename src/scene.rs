use starframe::{
    graph::{Graph, LayerViewMut},
    graphics::Shape,
    math::{self as m},
    physics::{Collider, Physics},
};

use assets_manager::{loader, Asset};

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
    StaticCapsuleChain { width: f64, points: Vec<[f64; 2]> },
}

impl Recipe {
    pub fn spawn(
        &self,
        physics: &mut Physics,
        (mut l_pose, mut l_coll, mut l_shape): (
            LayerViewMut<m::Pose>,
            LayerViewMut<Collider>,
            LayerViewMut<Shape>,
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
                    let mut shape = l_shape.insert(Shape::from_collider(coll.c, [1.0; 4]));
                    pose.connect(&mut coll);
                    pose.connect(&mut shape);
                }
            }
        }
    }
}
