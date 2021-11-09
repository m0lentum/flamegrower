use starframe::{self as sf, physics::Physics};

#[derive(Clone, Debug, serde::Deserialize)]
pub enum Recipe {
    Todo,
}

impl Recipe {
    pub fn spawn(&self, physics: &mut Physics, graph: &sf::graph::Graph) {
        match self {
            Recipe::Todo => {}
        }
    }
}
