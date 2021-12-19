//! Player controller

use starframe::{
    self as sf,
    graph::NodeKey,
    graphics as gx,
    input::KeyAxisState,
    math as m,
    physics::{self as phys, rope},
};

use crate::fire::Flammable;

// tuning constants

const COLL_R: f64 = 0.1;
const COLL_LENGTH: f64 = 0.2;
const GROUNDED_ANGLE_LIMIT: f64 = 60.0;
const BASE_MOVE_SPEED: f64 = 5.0;
const GROUND_ACCEL: f64 = 1.0;
const AIR_ACCEL: f64 = 0.2;
const ROPE_SWINGING_ACCEL: f64 = 0.1;
const JUMP_VEL: f64 = 6.0;
const RAY_START_OFFSET: f64 = 0.21;
const RAY_MAX_DISTANCE: f64 = 8.0;
const ROPE_MIN_LENGTH: f64 = 1.0;
const BOOST_ANGLE_LIMIT: f64 = 60.0;
const BOOST_BONUS_SPEED: f64 = 0.1;

// types

#[derive(Clone, Copy, Debug)]
pub struct PlayerController {
    body: Option<PlayerNodes>,
    attached_rope: Option<AttachedRope>,
}
#[derive(Clone, Copy, Debug)]
struct PlayerNodes {
    pose: NodeKey<m::Pose>,
    body: NodeKey<phys::Body>,
    coll: NodeKey<phys::Collider>,
}
#[derive(Clone, Copy, Debug)]
struct AttachedRope {
    rope: rope::RopeProperties,
    player_constraint: phys::ConstraintHandle,
}

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

        let mut l_pose = graph.get_layer_mut::<m::Pose>();
        let mut l_collider = graph.get_layer_mut::<phys::Collider>();
        let mut l_body = graph.get_layer_mut::<phys::Body>();
        let mut l_shape = graph.get_layer_mut::<gx::Shape>();

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
        keys: &crate::settings::PlayerKeys,
        physics: &mut phys::Physics,
        graph: &mut super::MyGraph,
    ) -> Option<()> {
        let nodes = self.body.as_ref()?;

        let mut l_pose = graph.get_layer_mut::<m::Pose>();
        let mut l_collider = graph.get_layer_mut::<phys::Collider>();
        let mut l_body = graph.get_layer_mut::<phys::Body>();
        let mut l_shape = graph.get_layer_mut::<gx::Shape>();
        let mut l_rope = graph.get_layer_mut::<rope::Rope>();
        let mut l_flammable = graph.get_layer_mut::<Flammable>();

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

        let target_hdir = match input.get_key_axis_state(keys.right, keys.left) {
            KeyAxisState::Zero => 0.0,
            KeyAxisState::Pos => 1.0,
            KeyAxisState::Neg => -1.0,
        };
        let target_vdir = match input.get_key_axis_state(keys.up, keys.down) {
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

        if input.is_key_pressed(keys.jump, Some(0)) {
            if let Groundedness::EvenGround(normal) = groundedness {
                player_body.velocity.linear -= JUMP_VEL * *normal;
            }
        } else if input.is_key_released(keys.jump, Some(0)) && player_body.velocity.linear.y > 0.0 {
            player_body.velocity.linear.y /= 2.0;
        }

        //
        // spawn and delete ropes
        //

        // TODO: this is a bit of a clumsy block of conditions,
        // probably refactor this a bit when implementing aim on press and shoot on release
        if matches!(
            (
                self.attached_rope,
                input.is_key_pressed(keys.aim_new, Some(0)),
                input.is_key_pressed(keys.aim_connect, Some(0)),
            ),
            (None, true, _) | (Some(_), _, true)
        ) {
            let ray_dir = if target_vdir == 0.0 && target_hdir == 0.0 {
                // shoot the rope upwards by default
                // (TODO instead: aiming mode when holding aim,
                // shoot in arrow key direction on release or cancel if no direction held)
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
                match self.attached_rope {
                    //
                    // new rope
                    //
                    None => {
                        if hit.t >= ROPE_MIN_LENGTH {
                            let player_pos = player_pose.translation;
                            // start at the other end to control angle constraint propagation
                            let rope_start = ray.point_at_t(hit.t - 0.05);
                            let rope_end = ray.point_at_t(0.05);
                            let rope = rope::spawn_line(
                                rope::Rope {
                                    ..Default::default()
                                },
                                rope_start,
                                rope_end,
                                (
                                    l_body.subview_mut(),
                                    l_pose.subview_mut(),
                                    l_collider.subview_mut(),
                                    l_rope.subview_mut(),
                                    l_shape.subview_mut(),
                                ),
                            );
                            // make it flammable
                            let mut iter =
                                rope::RopeIterMut::new(l_rope.get(rope.rope_node)?, &mut l_body);
                            while let Some(mut particle) = iter.next() {
                                let mut coll = particle.get_neighbor_mut(&mut l_collider)?;
                                let mut flammable = l_flammable.insert(Flammable::default());
                                flammable.connect(&mut coll);
                                flammable.connect(&mut particle);
                            }

                            // constraint on the player

                            let player_constraint = physics.add_constraint(
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

                            self.attached_rope = Some(AttachedRope {
                                rope,
                                player_constraint,
                            });

                            // adjust player velocity towards the circle around the attachment point
                            // for juicy swings

                            // reborrow needed to allow subviews of the layer above
                            let player_body = l_body.get_mut_unchecked(nodes.body).c;

                            let vel_mag = player_body.velocity.linear.mag();
                            let vel_dir =
                                m::Unit::new_unchecked(player_body.velocity.linear / vel_mag);

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
                                player_body.velocity.linear =
                                    (BOOST_BONUS_SPEED + vel_mag) * *tangent;
                            }
                        }
                    }
                    //
                    // disconnect existing rope from self and connect to whatever was hit
                    //
                    Some(attached) => {
                        physics.remove_constraint(attached.player_constraint);
                        self.attached_rope = None;

                        let curr_end_body = l_body.get(attached.rope.last_particle)?;
                        let curr_end = curr_end_body.get_neighbor(&l_pose.subview())?.c.translation;
                        let new_segment_end = ray.point_at_t(hit.t - 0.05);
                        let dir = m::Unit::new_normalize(new_segment_end - curr_end);

                        let mut rope_node = l_rope.get_mut(attached.rope.rope_node)?;
                        let dist = (new_segment_end - curr_end).mag();
                        let new_particle_count = (dist / rope_node.c.spacing) as usize;

                        let new_rope = rope::extend_line(
                            &mut rope_node,
                            dir,
                            new_particle_count,
                            (
                                l_body.subview_mut(),
                                l_pose.subview_mut(),
                                l_collider.subview_mut(),
                                l_shape.subview_mut(),
                            ),
                        );
                        // make the newly added part flammable
                        let mut iter =
                            rope::RopeIterMut::new(l_rope.get(new_rope.rope_node)?, &mut l_body);
                        while let Some(mut particle) = iter.next() {
                            if particle.get_neighbor_mut(&mut l_flammable).is_none() {
                                let mut coll = particle.get_neighbor_mut(&mut l_collider)?;
                                let mut flammable = l_flammable.insert(Flammable::default());
                                flammable.connect(&mut coll);
                                flammable.connect(&mut particle);
                            }
                        }

                        // constraint on the new target

                        let coll_hit = l_collider.get_unchecked(hit.collider);
                        match coll_hit.get_neighbor(&l_body.subview()) {
                            Some(body) => {
                                let offset = new_segment_end
                                    - body
                                        .get_neighbor(&l_pose.subview())
                                        .map(|p| p.c.translation)
                                        .unwrap_or_default();
                                physics.add_constraint(
                                    phys::ConstraintBuilder::new(new_rope.last_particle)
                                        .with_target(body.key())
                                        .with_target_origin(offset)
                                        .build_attachment(),
                                );
                            }
                            None => {
                                physics.add_constraint(
                                    phys::ConstraintBuilder::new(new_rope.last_particle)
                                        .with_target_origin(new_segment_end)
                                        .build_attachment(),
                                );
                            }
                        }
                    }
                }
            }
        }
        //
        // ignite (only if not simultaneously shooting)
        //
        else if let (true, Some(attached)) = (
            input.is_key_pressed(keys.ignite, Some(0)),
            self.attached_rope,
        ) {
            physics.remove_constraint(attached.player_constraint);
            self.attached_rope = None;

            let particle = l_body.get(attached.rope.last_particle)?;
            let flammable = particle.get_neighbor_mut(&mut l_flammable)?;
            flammable.c.ignite();
        }

        Some(())
    }
}
