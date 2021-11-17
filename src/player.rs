use starframe::{
    self as sf,
    graph::{LayerViewMut, NodeKey},
    graphics as gx,
    input::{Key, KeyAxisState},
    math as m, physics as phys,
};

pub struct PlayerController {
    entity: Option<PlayerNodes>,
}
struct PlayerNodes {
    pose: NodeKey<m::Pose>,
    body: NodeKey<phys::Body>,
    coll: NodeKey<phys::Collider>,
}

type Layers<'a> = (
    LayerViewMut<'a, m::Pose>,
    LayerViewMut<'a, phys::Collider>,
    LayerViewMut<'a, phys::Body>,
    LayerViewMut<'a, gx::Shape>,
);

impl PlayerController {
    pub fn new() -> Self {
        Self { entity: None }
    }

    pub fn respawn(&mut self, scene: &super::Scene, graph: &mut sf::graph::Graph) {
        if let Some(nodes) = &self.entity {
            graph.delete(nodes.body);
        }

        let (mut l_pose, mut l_collider, mut l_body, mut l_shape): Layers =
            graph.get_layer_bundle();

        let mut pose = l_pose.insert(m::Pose::new(scene.player_start, m::Angle::Deg(90.0).into()));
        const R: f64 = 0.1;
        const LENGTH: f64 = 0.2;
        let mut coll = l_collider.insert(phys::Collider::new_capsule(LENGTH, R).with_material(
            phys::Material {
                static_friction_coef: None,
                dynamic_friction_coef: None,
                restitution_coef: 0.0,
            },
        ));
        let mut shape = l_shape.insert(gx::Shape::from_collider(coll.c, [0.2, 0.8, 0.6, 1.0]));
        let mut body = l_body.insert(phys::Body::new_particle(1.0));
        pose.connect(&mut body);
        pose.connect(&mut coll);
        body.connect(&mut coll);
        pose.connect(&mut shape);

        self.entity = Some(PlayerNodes {
            pose: pose.key(),
            body: body.key(),
            coll: coll.key(),
        });
    }

    pub fn tick(
        &mut self,
        input: &sf::InputCache,
        physics: &mut phys::Physics,
        graph: &mut sf::graph::Graph,
    ) -> Option<()> {
        let nodes = self.entity.as_ref()?;

        let mut layers: Layers = graph.get_layer_bundle();
        let (ref mut l_pose, ref mut l_collider, ref mut l_body, ref mut _l_shape) = layers;

        let player_body = l_body.get_mut(nodes.body)?.c;
        let player_pose = l_pose.get(nodes.pose)?.c;

        // figure out if we're on the ground

        const GROUNDED_ANGLE_LIMIT: f64 = 60.0;
        let normal_y_limit = m::Angle::Deg(GROUNDED_ANGLE_LIMIT).rad().cos();

        let most_downright_contact = physics
            .contacts_for_collider(nodes.coll)
            .min_by(|c0, c1| c0.normal.y.partial_cmp(&c1.normal.y).unwrap());

        #[derive(Debug, Clone, Copy)]
        enum Groundedness {
            EvenGround(m::Unit<m::Vec2>),
            SteepSlope(m::Unit<m::Vec2>),
            Air,
        }
        let groundedness = match most_downright_contact {
            Some(cont) if cont.normal.y < -normal_y_limit => Groundedness::EvenGround(cont.normal),
            Some(cont) if cont.normal.y < 0.0 => Groundedness::SteepSlope(cont.normal),
            _ => Groundedness::Air,
        };

        //
        // move
        //

        let target_hdir = match input.get_key_axis_state(Key::Right, Key::Left) {
            KeyAxisState::Zero => 0.0,
            KeyAxisState::Pos => 1.0,
            KeyAxisState::Neg => -1.0,
        };
        let ground_dir = if let Groundedness::EvenGround(normal) = groundedness {
            m::left_normal(*normal)
        } else {
            m::Vec2::unit_x()
        };

        const BASE_MOVE_SPEED: f64 = 6.0;
        const GROUND_ACCEL: f64 = 3.0;
        const AIR_ACCEL: f64 = 1.0;

        let target_hvel = target_hdir * BASE_MOVE_SPEED;
        let accel_needed = target_hvel - player_body.velocity.linear.dot(ground_dir);
        let max_accel = match groundedness {
            Groundedness::EvenGround(_) => GROUND_ACCEL,
            Groundedness::SteepSlope(normal) if normal.x * target_hdir >= 0.0 => 0.0,
            _ => AIR_ACCEL,
        };
        let accel = if accel_needed.abs() <= max_accel {
            accel_needed
        } else {
            max_accel.copysign(accel_needed)
        };
        player_body.velocity.linear += accel * ground_dir;

        //
        // jump
        //

        const JUMP_VEL: f64 = 6.0;
        if input.is_key_pressed(Key::LShift, Some(0)) {
            if let Groundedness::EvenGround(normal) = groundedness {
                player_body.velocity.linear -= JUMP_VEL * *normal;
            }
        } else if input.is_key_released(Key::LShift, Some(0)) && player_body.velocity.linear.y > 0.0
        {
            player_body.velocity.linear.y /= 2.0;
        }

        //
        // spawn ropes
        //

        if input.is_key_pressed(Key::Z, Some(0)) {
            let target_ydir = match input.get_key_axis_state(Key::Up, Key::Down) {
                KeyAxisState::Zero => 0.0,
                KeyAxisState::Pos => 1.0,
                KeyAxisState::Neg => -1.0,
            };
            let ray_dir = if target_ydir == 0.0 && target_hdir == 0.0 {
                // TODO: current facing of the player here
                m::Unit::unit_x()
            } else {
                m::Unit::new_normalize(m::Vec2::new(target_hdir, target_ydir))
            };

            const RAY_START_OFFSET: f64 = 0.21;
            const RAY_MAX_DISTANCE: f64 = 8.0;
            const ROPE_MIN_LENGTH: f64 = 1.0;

            let ray = phys::Ray {
                start: player_pose.translation + *ray_dir * RAY_START_OFFSET,
                dir: ray_dir,
            };
            if let Some(hit) = physics.raycast(
                ray,
                RAY_MAX_DISTANCE,
                (l_pose.as_view(), l_collider.as_view()),
            ) {
                if hit.t >= ROPE_MIN_LENGTH {
                    drop(layers);
                    // start at the other end to control angle constraint propagation
                    let rope_start = ray.point_at_t(hit.t - 0.05);
                    let rope_end = ray.point_at_t(0.05);
                    let rope = phys::spawn_rope_line(
                        phys::Rope {
                            ..Default::default()
                        },
                        rope_start,
                        rope_end,
                        graph.get_layer_bundle(),
                    );

                    // we had to drop and re-lock layers because current layerbundle impl requires move.
                    // TODO: figure out something to prevent this on the starframe side
                    let mut layers: Layers = graph.get_layer_bundle();
                    let (ref mut l_pose, ref mut l_collider, ref mut l_body, ref mut _l_shape) =
                        layers;
                    let coll = l_collider.get_unchecked(hit.collider);
                    let l_body_view = l_body.as_view();
                    match coll.get_neighbor(&l_body_view) {
                        Some(body) => {
                            let offset = rope_start
                                - body
                                    .get_neighbor(&l_pose.as_view())
                                    .map(|p| p.c.translation)
                                    .unwrap_or_default();
                            physics.add_constraint(
                                phys::ConstraintBuilder::new(rope.first_particle)
                                    .with_target(body.key())
                                    .with_target_origin(offset)
                                    .build_attachment(),
                            );
                        }
                        None => {
                            physics.add_constraint(
                                phys::ConstraintBuilder::new(rope.first_particle)
                                    .with_target_origin(rope_start)
                                    .build_attachment(),
                            );
                        }
                    }
                }
            }
        }

        Some(())
    }
}
