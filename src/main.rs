use starframe::{
    game::{self, Game},
    graph::{self, make_graph},
    graphics as gx,
    input::{Key, MouseButton},
    math::{self as m, uv},
    physics as phys,
};

mod player;
mod recipes;
use recipes::Recipe;

fn main() {
    let game = Game::init(
        60,
        winit::window::WindowBuilder::new()
            .with_title("Floramancer")
            .with_inner_size(winit::dpi::LogicalSize {
                width: 800.0,
                height: 600.0,
            }),
    );
    let state = State::init(&game.renderer);
    game.run(state);
}

//
// Types
//

enum StateEnum {
    Playing,
    Paused,
}
pub struct State {
    // systems
    graph: graph::Graph,
    physics: phys::Physics,
    camera: gx::camera::MouseDragCamera,
    shape_renderer: gx::ShapeRenderer,
    // content
    state: StateEnum,
    scene: Scene,
    player: player::PlayerController,
}
impl State {
    fn init(renderer: &gx::Renderer) -> Self {
        State {
            graph: make_graph! {},
            physics: phys::Physics::new(
                phys::TuningConstants {
                    ..Default::default()
                },
                phys::collision::HGridParams {
                    approx_bounds: phys::collision::AABB {
                        min: m::Vec2::new(-40.0, -15.0),
                        max: m::Vec2::new(40.0, 25.0),
                    },
                    lowest_spacing: 0.5,
                    level_count: 2,
                    spacing_ratio: 3,
                    initial_capacity: 600,
                },
            ),
            camera: gx::camera::MouseDragCamera::new(
                gx::camera::ScalingStrategy::ConstantDisplayArea {
                    width: 20.0,
                    height: 10.0,
                },
            ),
            shape_renderer: gx::ShapeRenderer::new(renderer),
            //
            state: StateEnum::Playing,
            scene: Scene::default(),
            player: player::PlayerController::new(),
        }
    }

    fn reset(&mut self) {
        self.physics.clear_constraints();
        self.graph.reset();
    }

    fn read_scene(&mut self, file_idx: usize) {
        let dir = std::fs::read_dir("./assets/scenes");
        match dir {
            Err(err) => eprintln!("Scenes dir not found: {}", err),
            Ok(mut dir) => {
                if let Some(Ok(entry)) = dir.nth(file_idx) {
                    let file = std::fs::File::open(entry.path());
                    match file {
                        Ok(file) => {
                            let scene = Scene::read_from_file(file);
                            match scene {
                                Err(err) => eprintln!("Failed to parse file: {}", err),
                                Ok(scene) => self.scene = scene,
                            }
                        }
                        Err(err) => eprintln!("Failed to open file: {}", err),
                    }
                }
            }
        }
    }

    fn instantiate_scene(&mut self) {
        self.scene.instantiate(&mut self.physics, &self.graph);
    }
}

#[derive(Clone, Copy, Debug)]
pub enum MouseMode {
    /// Grab objects with the mouse
    Grab,
    /// Move the camera with the mouse
    Camera,
}

/// The recipes in a scene plus some adjustable parameters.
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

impl Scene {
    pub fn read_from_file(file: std::fs::File) -> Result<Self, ron::de::Error> {
        use serde::Deserialize;
        use std::io::Read;

        let mut reader = std::io::BufReader::new(file);
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;

        let mut deser = ron::de::Deserializer::from_bytes(bytes.as_slice())?;
        Scene::deserialize(&mut deser)
    }

    pub fn instantiate(&self, physics: &mut phys::Physics, graph: &graph::Graph) {
        for recipe in &self.recipes {
            recipe.spawn(physics, graph);
        }
    }
}

//
// State updates
//

impl game::GameState for State {
    fn tick(&mut self, dt: f64, game: &Game) -> Option<()> {
        // exit on esc
        if game.input.is_key_pressed(Key::Escape, None) {
            return None;
        }

        // mouse camera
        self.camera
            .update(&game.input, game.renderer.window_size().into());
        if (game.input).is_mouse_button_pressed(MouseButton::Middle, Some(0)) {
            self.camera.pose = uv::DSimilarity2::identity();
        }

        // reload scene
        if game.input.is_key_pressed(Key::Return, Some(0)) {
            self.reset();
            self.instantiate_scene();
        }

        match self.state {
            StateEnum::Playing => {
                if game.input.is_key_pressed(Key::P, Some(0)) {
                    self.state = StateEnum::Paused;
                    return Some(());
                }

                // respawn player
                if game.input.is_key_pressed(Key::R, Some(0)) {
                    self.player.respawn(&mut self.graph);
                }

                {
                    let grav = phys::forcefield::Gravity(m::Vec2::new(0.0, -9.81));
                    self.physics.tick(dt, &grav, self.graph.get_layer_bundle());
                }
                {
                    self.player
                        .tick(&game.input, &self.physics, &mut self.graph);
                }

                Some(())
            }
            StateEnum::Paused => {
                if game.input.is_key_pressed(Key::P, Some(0)) {
                    self.state = StateEnum::Playing;
                    return Some(());
                }

                Some(())
            }
        }
    }

    fn draw(&mut self, renderer: &mut gx::Renderer) {
        let mut ctx = renderer.draw_to_window();
        ctx.clear(wgpu::Color {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1.0,
        });

        self.shape_renderer
            .draw(&self.camera, &mut ctx, self.graph.get_layer_bundle());

        ctx.submit();
    }
}
