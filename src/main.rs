use lazy_static::lazy_static;
use starframe::{
    game::{self, Game},
    graph::make_graph,
    graphics as gx,
    input::{Key, MouseButton},
    math::{self as m, uv},
    physics as phys,
};

use assets_manager::{AssetCache, Handle};

mod player;
mod scene;
use scene::Scene;

mod collision_layers {
    use starframe::physics::collision::{MaskMatrix, ROPE_LAYER};

    pub const PLAYER: u64 = 1;

    pub(super) fn create_layer_matrix() -> MaskMatrix {
        let mut mat = MaskMatrix::default();
        mat.ignore(PLAYER, ROPE_LAYER);
        mat
    }
}

//

lazy_static! {
    static ref ASSETS: AssetCache = AssetCache::new("assets").expect("assets directory not found");
}
pub type AssetHandle<T> = Handle<'static, T>;

fn main() {
    #[cfg(debug_assertions)]
    ASSETS.enhance_hot_reloading();

    use winit::platform::unix::WindowBuilderExtUnix;
    let game = Game::init(
        60,
        winit::window::WindowBuilder::new()
            .with_title("Floramancer")
            .with_inner_size(winit::dpi::LogicalSize {
                width: 1280.0,
                height: 720.0,
            })
            // X11 class I use for a window manager rule
            .with_class("game".into(), "game".into()),
    );
    let state = State::init(&game.renderer);
    game.run(state);
}

//
// State types
//

type MyGraph = make_graph! {};

enum StateEnum {
    Playing,
    Paused,
}
pub struct State {
    // systems
    graph: MyGraph,
    physics: phys::Physics,
    camera: gx::camera::MouseDragCamera,
    shape_renderer: gx::ShapeRenderer,
    debug_visualizer: gx::DebugVisualizer,
    grid_vis_active: bool,
    // content
    state: StateEnum,
    scene: AssetHandle<Scene>,
    player: player::PlayerController,
}
impl State {
    fn init(renderer: &gx::Renderer) -> Self {
        let scene = ASSETS
            .load::<Scene>("scenes.test")
            .expect("test scene failed to load");

        State {
            graph: MyGraph::new(),
            physics: phys::Physics::new(
                phys::TuningConstants {
                    ..Default::default()
                },
                phys::collision::HGridParams {
                    approx_bounds: phys::collision::AABB {
                        min: m::Vec2::new(-10.0, -6.0),
                        max: m::Vec2::new(10.0, 6.0),
                    },
                    lowest_spacing: 0.6,
                    level_count: 3,
                    spacing_ratio: 2,
                    initial_capacity: 600,
                },
                collision_layers::create_layer_matrix(),
            ),
            camera: gx::camera::MouseDragCamera::new(
                gx::camera::ScalingStrategy::ConstantDisplayArea {
                    width: 20.0,
                    height: 10.0,
                },
            ),
            shape_renderer: gx::ShapeRenderer::new(renderer),
            debug_visualizer: gx::DebugVisualizer::new(renderer),
            grid_vis_active: false,
            //
            state: StateEnum::Playing,
            scene,
            player: player::PlayerController::new(),
        }
    }

    fn reset(&mut self) {
        self.physics.clear_constraints();
        self.graph.reset();
    }

    fn instantiate_scene(&mut self) {
        self.scene
            .read()
            .instantiate(&mut self.physics, &self.graph);
    }
}

//
// State updates
//

impl game::GameState for State {
    fn tick(&mut self, dt: f64, game: &Game) -> Option<()> {
        // exit on esc for now
        if game.input.is_key_pressed(Key::Escape, None) {
            return None;
        }

        #[cfg(debug_assertions)]
        ASSETS.hot_reload();

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

        // toggle debug visualization
        if game.input.is_key_pressed(Key::G, Some(0)) {
            self.grid_vis_active = !self.grid_vis_active;
        }

        match self.state {
            StateEnum::Playing => {
                if game.input.is_key_pressed(Key::P, Some(0)) {
                    self.state = StateEnum::Paused;
                    return Some(());
                }

                // respawn player
                if game.input.is_key_pressed(Key::R, Some(0)) {
                    self.player.respawn(&*self.scene.read(), &mut self.graph);
                }

                {
                    let grav = phys::forcefield::Gravity(m::Vec2::new(0.0, -9.81));
                    self.physics.tick(dt, &grav, self.graph.get_layer_bundle());
                }
                {
                    self.player
                        .tick(&game.input, &mut self.physics, &mut self.graph);
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

        if self.grid_vis_active {
            self.debug_visualizer
                .draw_spatial_index(&self.physics, &self.camera, &mut ctx);
        }

        ctx.submit();
    }
}
