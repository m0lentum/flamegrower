use lazy_static::lazy_static;
use starframe::{
    game::{self, Game},
    graph::{new_graph, Graph},
    graphics as gx,
    input::MouseButton,
    math::{self as m, uv},
    physics as phys,
};

use assets_manager::{AssetCache, Handle};

mod fire;
mod player;
mod scene;
use scene::Scene;
mod settings;
use settings::Settings;

//
// Constants & init
//

fn world_graph() -> Graph {
    new_graph! {
        // starframe types
        m::Pose,
        phys::Body,
        phys::Collider,
        phys::rope::Rope,
        gx::Mesh,
        // our types
        fire::Flammable,
        player::PlayerSpawnPoint,
        player::Interactable,
    }
}

mod collision_layers {
    use starframe::physics::collision::{MaskMatrix, ROPE_LAYER};

    pub const PLAYER: usize = 1;
    /// Things that are only interacted with by the player
    pub const INTERACTABLE: usize = 2;

    pub(super) fn create_layer_matrix() -> MaskMatrix {
        let mut mat = MaskMatrix::default();
        mat.ignore(PLAYER, ROPE_LAYER);
        mat.ignore_all(INTERACTABLE);
        mat.unignore(INTERACTABLE, PLAYER);
        mat
    }
}

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
            .with_title("Flamegrower")
            // X11 class I use for a window manager rule
            .with_class("game".into(), "game".into())
            .with_inner_size(winit::dpi::LogicalSize {
                width: 1280.0,
                height: 720.0,
            }),
    );
    let state = State::init(&game.renderer);
    game.run(state, None);
}

//
// State types
//

enum StateEnum {
    Playing,
    Paused,
}
pub struct State {
    // systems
    graph: Graph,
    physics: phys::Physics,
    camera: gx::camera::MouseDragCamera,
    mesh_renderer: gx::MeshRenderer,
    outline_renderer: gx::OutlineRenderer,
    debug_visualizer: gx::DebugVisualizer,
    grid_vis_active: bool,
    // content
    settings: AssetHandle<Settings>,
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
            graph: world_graph(),
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
                    width: 30.0,
                    height: 15.0,
                },
            ),
            mesh_renderer: gx::MeshRenderer::new(renderer),
            outline_renderer: gx::OutlineRenderer::new(
                gx::OutlineParams {
                    thickness: 10,
                    shape: gx::OutlineShape::octagon(),
                },
                renderer,
            ),
            debug_visualizer: gx::DebugVisualizer::new(renderer),
            grid_vis_active: false,
            //
            settings: ASSETS.load("settings").expect("settings failed to load"),
            state: StateEnum::Playing,
            scene,
            player: player::PlayerController::new(),
        }
    }

    fn reset(&mut self) {
        self.physics.reset();
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
        let settings = self.settings.read();
        let keys = settings.keymap;

        // while we don't have a real menu, just exit the game on keypress
        if game.input.is_key_pressed(keys.menus.exit, None) {
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
        if game.input.is_key_pressed(keys.menus.reload, Some(0)) {
            self.reset();
            self.instantiate_scene();
        }

        // toggle debug visualization
        if game.input.is_key_pressed(keys.debug.toggle_grid, Some(0)) {
            self.grid_vis_active = !self.grid_vis_active;
        }

        match self.state {
            StateEnum::Playing => {
                if game.input.is_key_pressed(keys.menus.pause, Some(0)) {
                    self.state = StateEnum::Paused;
                    return Some(());
                }

                // respawn player
                if game.input.is_key_pressed(keys.player.respawn, Some(0)) {
                    self.player.respawn(&mut self.graph);
                }

                let grav = phys::forcefield::Gravity(m::Vec2::new(0.0, -9.81));
                self.physics.tick(dt, &grav, self.graph.get_layer_bundle());

                self.player.tick(
                    &game.input,
                    &keys.player,
                    &mut self.physics,
                    &mut self.graph,
                );

                fire::tick(dt, &mut self.physics, &mut self.graph);

                Some(())
            }
            StateEnum::Paused => {
                if game.input.is_key_pressed(keys.menus.pause, Some(0)) {
                    self.state = StateEnum::Playing;
                    return Some(());
                }

                Some(())
            }
        }
    }

    fn draw(&mut self, renderer: &mut gx::Renderer) {
        self.outline_renderer.prepare(renderer);
        self.outline_renderer
            .init_meshes(&self.camera, renderer, self.graph.get_layer_bundle());
        self.outline_renderer.compute(renderer);

        let mut ctx = renderer.draw_to_window();
        ctx.clear(wgpu::Color {
            r: 0.1,
            g: 0.1,
            b: 0.1,
            a: 1.0,
        });

        self.outline_renderer.draw(&mut ctx);

        self.mesh_renderer
            .draw(&self.camera, &mut ctx, self.graph.get_layer_bundle());

        if self.grid_vis_active {
            self.debug_visualizer
                .draw_spatial_index(&self.physics, &self.camera, &mut ctx);
        }

        ctx.submit();
    }
}
