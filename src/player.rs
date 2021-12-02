use starframe::{
    self as sf,
    graph::{LayerViewMut, NodeKey},
    graphics as gx,
    input::{Key, KeyAxisState},
    math as m, physics as phys,
};

// tuning constants

const COLL_R: f64 = 0.1;
const COLL_LENGTH: f64 = 0.2;
const GROUNDED_ANGLE_LIMIT: f64 = 60.0;
const BASE_MOVE_SPEED: f64 = 6.0;
const GROUND_ACCEL: f64 = 3.0;
const AIR_ACCEL: f64 = 1.0;
const ROPE_SWINGING_ACCEL: f64 = 0.1;
const JUMP_VEL: f64 = 6.0;
const RAY_START_OFFSET: f64 = 0.21;
const RAY_MAX_DISTANCE: f64 = 8.0;
const ROPE_MIN_LENGTH: f64 = 1.0;
const BOOST_ANGLE_LIMIT: f64 = 60.0;
const BOOST_BONUS_SPEED: f64 = 0.2;

// types

pub struct PlayerController {
    body: Option<PlayerNodes>,
    attached_rope: Option<NodeKey<phys::Rope>>,
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
        Self {
            body: None,
            attached_rope: None,
        }
    }

    pub fn respawn(&mut self, scene: &super::Scene, graph: &mut super::MyGraph) {
        if let Some(nodes) = &self.body {
            graph.delete(nodes.body);
        }

        let (mut l_pose, mut l_collider, mut l_body, mut l_shape): Layers =
            graph.get_layer_bundle();

        let mut pose = l_pose.insert(m::Pose::new(scene.player_start, m::Angle::Deg(90.0).into()));
        let mut coll = l_collider.insert(
            phys::Collider::new_capsule(COLL_LENGTH, COLL_R)
                .with_material(phys::Material {
                    static_friction_coef: None,
                    dynamic_friction_coef: None,
                    restitution_coef: 0.0,
                })
                .with_layer(super::collision_layers::PLAYER),
        );
        let mut shape = l_shape.insert(gx::Shape::from_collider(coll.c, [0.2, 0.8, 0.6, 1.0]));
        let mut body = l_body.insert(phys::Body::new_particle(1.0));
        pose.connect(&mut body);
        pose.connect(&mut coll);
        body.connect(&mut coll);
        pose.connect(&mut shape);

        self.body = Some(PlayerNodes {
            pose: pose.key(),
            body: body.key(),
            coll: coll.key(),
        });
    }

    pub fn tick(
        &mut self,
        input: &sf::InputCache,
        physics: &mut phys::Physics,
        graph: &mut super::MyGraph,
    ) -> Option<()> {
        let nodes = self.body.as_ref()?;

        let (mut l_pose, mut l_collider, mut l_body, mut l_shape): Layers =
            graph.get_layer_bundle();

        let player_body = l_body.get_mut(nodes.body)?.c;
        let player_pose = l_pose.get(nodes.pose)?.c;

        // figure out if we're on the ground

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
        let target_vdir = match input.get_key_axis_state(Key::Up, Key::Down) {
            KeyAxisState::Zero => 0.0,
            KeyAxisState::Pos => 1.0,
            KeyAxisState::Neg => -1.0,
        };

        match (groundedness, self.attached_rope) {
            // special acceleration-based controls for in air with a rope
            // for improved swing feel and control, hopefully
            (Groundedness::Air, Some(_rope)) => {
                if target_hdir != 0.0 || target_vdir != 0.0 {
                    let target_dir = m::Unit::new_normalize(m::Vec2::new(target_hdir, target_vdir));
                    player_body.velocity.linear += ROPE_SWINGING_ACCEL * *target_dir;
                }
            }
            // normal controls for all other situations
            _ => {
                let ground_dir = match groundedness {
                    // on ground, match ground slope
                    Groundedness::EvenGround(normal) => m::left_normal(*normal),
                    // in air or on a steep slope, normal horizontal air acceleration
                    _ => m::Vec2::unit_x(),
                };

                let target_hvel = target_hdir * BASE_MOVE_SPEED;
                let accel_needed = target_hvel - player_body.velocity.linear.dot(ground_dir);
                let max_accel = match groundedness {
                    Groundedness::EvenGround(_) => GROUND_ACCEL,
                    // prevent movement up a steep slope
                    Groundedness::SteepSlope(normal) if normal.x * target_hdir >= 0.0 => 0.0,
                    _ => AIR_ACCEL,
                };
                let accel = if accel_needed.abs() <= max_accel {
                    accel_needed
                } else {
                    max_accel.copysign(accel_needed)
                };
                player_body.velocity.linear += accel * ground_dir;
            }
        }

        //
        // jump
        //

        if input.is_key_pressed(Key::LShift, Some(0)) {
            if let Groundedness::EvenGround(normal) = groundedness {
                player_body.velocity.linear -= JUMP_VEL * *normal;
            }
        } else if input.is_key_released(Key::LShift, Some(0)) && player_body.velocity.linear.y > 0.0
        {
            player_body.velocity.linear.y /= 2.0;
        }

        //
        // spawn and delete ropes
        //

        match (input.is_key_pressed(Key::Z, Some(0)), self.attached_rope) {
            (true, Some(rope)) => {
                // need to drop the layer views manually to be able to delete here
                drop(l_pose);
                drop(l_body);
                drop(l_collider);
                drop(l_shape);
                graph.delete(rope);
                self.attached_rope = None;
            }
            (true, None) => {
                let ray_dir = if target_vdir == 0.0 && target_hdir == 0.0 {
                    // shoot the rope upwards by default
                    m::Unit::unit_y()
                } else {
                    m::Unit::new_normalize(m::Vec2::new(target_hdir, target_vdir))
                };

                let ray = phys::Ray {
                    start: player_pose.translation + *ray_dir * RAY_START_OFFSET,
                    dir: ray_dir,
                };
                if let Some(hit) = physics.raycast(
                    ray,
                    RAY_MAX_DISTANCE,
                    (l_pose.subview(), l_collider.subview()),
                ) {
                    if hit.t >= ROPE_MIN_LENGTH {
                        let player_pos = player_pose.translation;
                        // start at the other end to control angle constraint propagation
                        let rope_start = ray.point_at_t(hit.t - 0.05);
                        let rope_end = ray.point_at_t(0.05);
                        let rope = phys::spawn_rope_line(
                            phys::Rope {
                                ..Default::default()
                            },
                            rope_start,
                            rope_end,
                            (
                                l_body.subview_mut(),
                                l_pose.subview_mut(),
                                l_collider.subview_mut(),
                                graph.get_layer_mut(),
                                l_shape.subview_mut(),
                            ),
                        );

                        self.attached_rope = Some(rope.rope_node);

                        // constraint on the player

                        physics.add_constraint(
                            phys::ConstraintBuilder::new(nodes.body)
                                .with_target(rope.last_particle)
                                .with_limit(phys::ConstraintLimit::Lt)
                                .build_distance((rope_end - player_pos).mag()),
                        );

                        // constraint on the target

                        let coll_hit = l_collider.get_unchecked(hit.collider);
                        match coll_hit.get_neighbor(&l_body.subview()) {
                            Some(body) => {
                                let offset = rope_start
                                    - body
                                        .get_neighbor(&l_pose.subview())
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

                        // adjust player velocity towards the circle around the attachment point
                        // for juicy swings

                        // reborrow needed to allow subviews of the layer above
                        let player_body = l_body.get_mut_unchecked(nodes.body).c;

                        let vel_mag = player_body.velocity.linear.mag();
                        let vel_dir = m::Unit::new_unchecked(player_body.velocity.linear / vel_mag);

                        let player_to_center = rope_start - player_pos;
                        let tangent = m::Unit::new_normalize(m::left_normal(player_to_center));
                        let tan_dot_vel = tangent.dot(*vel_dir);
                        let (tangent, tan_dot_vel) = if tan_dot_vel >= 0.0 {
                            (tangent, tan_dot_vel)
                        } else {
                            (-tangent, -tan_dot_vel)
                        };

                        let dot_limit = m::Angle::Deg(BOOST_ANGLE_LIMIT).rad().cos();
                        if tan_dot_vel > dot_limit {
                            player_body.velocity.linear = (BOOST_BONUS_SPEED + vel_mag) * *tangent;
                        }
                    }
                }
            }
            (false, _) => (),
        }

        Some(())
    }
}
