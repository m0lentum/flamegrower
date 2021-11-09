use starframe::{
    self as sf,
    graph::{LayerViewMut, NodeKey},
    graphics as gx,
    input::{Key, KeyAxisState},
    math as m, physics as phys,
};

#[derive(Clone, Copy, Debug, Default, serde::Deserialize)]
#[serde(default)]
pub struct PlayerSpawnPoint {
    pub position: [f64; 2],
}

impl PlayerSpawnPoint {}

pub struct PlayerController {
    base_move_speed: f64,
    max_acceleration: f64,
    pub spawn_point: m::Vec2,
    entity: Option<PlayerNodes>,
}
struct PlayerNodes {
    body: NodeKey<phys::Body>,
}

type Layers<'a> = (
    LayerViewMut<'a, m::Pose>,
    LayerViewMut<'a, phys::Collider>,
    LayerViewMut<'a, phys::Body>,
    LayerViewMut<'a, gx::Shape>,
);
impl PlayerController {
    pub fn new() -> Self {
        Self {
            base_move_speed: 4.0,
            max_acceleration: 8.0,
            spawn_point: m::Vec2::new(0.0, 0.0),
            entity: None,
        }
    }

    pub fn respawn(&mut self, graph: &mut sf::graph::Graph) {
        if let Some(nodes) = &self.entity {
            graph.delete(nodes.body);
        }

        let (mut l_pose, mut l_collider, mut l_body, mut l_shape): Layers =
            graph.get_layer_bundle();
        const R: f64 = 0.1;
        const LENGTH: f64 = 0.2;

        let coll = phys::Collider::new_capsule(LENGTH, R).with_material(phys::Material {
            static_friction_coef: None,
            dynamic_friction_coef: None,
            restitution_coef: 0.0,
        });

        let mut pose_node = l_pose.insert(
            m::PoseBuilder::new()
                .with_position(self.spawn_point)
                .with_rotation(m::Angle::Deg(90.0))
                .build(),
        );
        let mut shape_node = l_shape.insert(gx::Shape::from_collider(&coll, [0.2, 0.8, 0.6, 1.0]));
        let mut coll_node = l_collider.insert(coll);
        let mut body_node = l_body.insert(phys::Body::new_particle(1.0));
        pose_node.connect(&mut body_node);
        pose_node.connect(&mut coll_node);
        body_node.connect(&mut coll_node);
        pose_node.connect(&mut shape_node);

        self.entity = Some(PlayerNodes {
            body: body_node.key(),
        });
    }

    pub fn tick(
        &mut self,
        input: &sf::InputCache,
        physics: &phys::Physics,
        graph: &mut sf::graph::Graph,
    ) -> Option<()> {
        let nodes = self.entity.as_ref()?;

        let (ref mut _l_pose, ref mut _l_collider, ref mut l_body, ref mut _l_shape): Layers =
            graph.get_layer_bundle();

        let player_body = l_body.get_mut(nodes.body)?.c;

        let target_hdir = match input.get_key_axis_state(Key::Right, Key::Left) {
            KeyAxisState::Zero => 0.0,
            KeyAxisState::Pos => 1.0,
            KeyAxisState::Neg => -1.0,
        };

        // move

        let move_speed = self.base_move_speed;

        let target_hvel = target_hdir * move_speed;
        let accel_needed = target_hvel - player_body.velocity.linear.x;
        let accel = accel_needed.min(self.max_acceleration);
        player_body.velocity.linear.x += accel;

        // jump

        if input.is_key_pressed(Key::LShift, Some(0)) {
            // TODO: only on ground, double jump, more snappy curve
            // (reject gravity and substitute my own)
            player_body.velocity.linear.y = 4.0;
        }

        Some(())
    }
}
