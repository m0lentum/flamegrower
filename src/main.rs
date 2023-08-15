use assets_manager::{AssetCache, Handle};
use lazy_static::lazy_static;
use starframe as sf;

mod fire;
mod player;
mod scene;
use scene::Scene;
mod settings;
use settings::Settings;

//
// Constants & init
//

mod collision_layers {
    use sf::physics::collision::ROPE_LAYER;
    use starframe as sf;

    pub const PLAYER: usize = 1;
    /// Things that are only interacted with by the player
    pub const INTERACTABLE: usize = 2;

    pub(super) fn create_layer_matrix() -> sf::CollisionMaskMatrix {
        let mut mat = sf::CollisionMaskMatrix::default();
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

    let window = sf::winit::window::WindowBuilder::new()
        .with_title("Flamegrower")
        .with_inner_size(sf::winit::dpi::LogicalSize {
            width: 1280.0,
            height: 720.0,
        });

    sf::Game::run::<State>(sf::GameParams {
        window,
        ..Default::default()
    });
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
    world: sf::hecs::World,
    physics: sf::PhysicsWorld,
    hecs_sync: sf::HecsSyncManager,
    camera: sf::Camera,
    camera_ctl: sf::MouseDragCameraController,
    mesh_renderer: sf::MeshRenderer,
    debug_visualizer: sf::DebugVisualizer,
    grid_vis_active: bool,
    // content
    settings: AssetHandle<Settings>,
    state: StateEnum,
    scene: AssetHandle<Scene>,
    player: player::PlayerController,
}
impl State {
    fn init(renderer: &sf::Renderer) -> Self {
        let scene = ASSETS
            .load::<Scene>("scenes.test")
            .expect("test scene failed to load");

        State {
            world: sf::hecs::World::new(),
            physics: sf::PhysicsWorld::new(
                sf::physics::TuningConstants {
                    ..Default::default()
                },
                collision_layers::create_layer_matrix(),
            ),
            hecs_sync: sf::HecsSyncManager::new_autosync(sf::HecsSyncOptions::both_ways()),
            camera: sf::Camera::new(sf::CameraScalingStrategy::ConstantDisplayArea {
                width: 30.0,
                height: 15.0,
            }),
            camera_ctl: sf::MouseDragCameraController {
                activate_button: sf::MouseButton::Middle.into(),
                reset_button: None,
                ..Default::default()
            },
            mesh_renderer: sf::MeshRenderer::new(renderer),
            debug_visualizer: sf::DebugVisualizer::new(renderer),
            grid_vis_active: false,
            //
            settings: ASSETS.load("settings").expect("settings failed to load"),
            state: StateEnum::Playing,
            scene,
            player: player::PlayerController::new(),
        }
    }

    fn reset(&mut self) {
        self.physics.clear();
        self.world.clear();
        self.camera.transform = sf::Transform::identity();
    }

    fn instantiate_scene(&mut self) {
        self.scene
            .read()
            .instantiate(&mut self.camera, &mut self.physics, &mut self.world);
    }
}

//
// State updates
//

impl sf::GameState for State {
    fn init(renderer: &sf::Renderer) -> Self {
        Self::init(renderer)
    }

    fn tick(&mut self, game: &sf::Game) -> Option<()> {
        let settings = self.settings.read();
        let keys = settings.keymap;

        // while we don't have a real menu, just exit the game on keypress
        if game.input.button(keys.menus.exit.into()) {
            return None;
        }

        #[cfg(debug_assertions)]
        ASSETS.hot_reload();

        // reload scene
        if game.input.button(keys.menus.reload.into()) {
            self.reset();
            self.instantiate_scene();
        }

        // toggle debug visualization
        if game.input.button(keys.debug.toggle_grid.into()) {
            self.grid_vis_active = !self.grid_vis_active;
        }

        self.camera_ctl.update(&mut self.camera, &game.input);

        match self.state {
            StateEnum::Playing => {
                if game.input.button(keys.menus.pause.into()) {
                    self.state = StateEnum::Paused;
                    return Some(());
                }

                // respawn player
                if game.input.button(keys.player.respawn.into()) {
                    self.player.respawn(&mut self.physics, &mut self.world);
                }

                self.hecs_sync
                    .sync_hecs_to_physics(&mut self.physics, &mut self.world);

                let grav = sf::forcefield::Gravity(sf::Vec2::new(0.0, -9.81));
                self.physics
                    .tick(game.dt_fixed, self.player.time_scale(), &grav);

                self.player.tick(
                    &game.input,
                    &mut self.camera,
                    &keys.player,
                    &mut self.physics,
                    &mut self.world,
                );

                self.hecs_sync
                    .sync_physics_to_hecs(&self.physics, &mut self.world);

                fire::tick(
                    game.dt_fixed,
                    &mut self.physics,
                    &mut self.world,
                    &mut self.hecs_sync,
                );

                Some(())
            }
            StateEnum::Paused => {
                if game.input.button(keys.menus.pause.into()) {
                    self.state = StateEnum::Playing;
                    return Some(());
                }

                Some(())
            }
        }
    }

    fn draw(&mut self, renderer: &mut sf::Renderer, dt: f32) {
        let mut ctx = renderer.draw_to_window();
        ctx.clear(sf::wgpu::Color {
            r: 0.00802,
            g: 0.0137,
            b: 0.02732,
            a: 1.0,
        });

        sf::animator::step_time(
            dt * self.player.time_scale().unwrap_or(1.0) as f32,
            &mut self.world,
        );
        sf::animator::update_joints(&mut self.world);

        self.mesh_renderer
            .draw(&self.camera, &mut ctx, &mut self.world);

        ctx.submit();

        let mut ctx = renderer.draw_to_window();

        if self.grid_vis_active {
            self.debug_visualizer
                .draw_bvh(20, &self.physics, &self.camera, &mut ctx);
        }

        ctx.submit();

        renderer.present_frame();
    }
}
