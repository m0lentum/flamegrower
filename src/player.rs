//! Player controller

use starframe::{
    self as sf,
    graph::{Graph, NodeKey},
    graphics as gx,
    input::{AxisQuery, ButtonQuery},
    math as m,
    physics::{self as phys, rope},
};

use crate::fire::Flammable;

// tuning constants

const COLL_R: f64 = 0.2;
const COLL_LENGTH: f64 = 0.4;
const PLAYER_MASS: f64 = 1.0;
const GROUNDED_ANGLE_LIMIT: f64 = 60.0;
const BASE_MOVE_SPEED: f64 = 6.0;
const GROUND_ACCEL: f64 = 1.0;
const AIR_ACCEL: f64 = 0.3;
const ROPE_SWINGING_ACCEL: f64 = 0.1;
const JUMP_VEL: f64 = 8.0;
const ROPE_START_OFFSET: f64 = 0.25;
const ROPE_MAX_LENGTH: f64 = 8.0;
const SPHERECAST_RADIUS: f64 = 0.1;
const ROPE_MIN_LENGTH: f64 = 1.0;
const BOOST_ANGLE_LIMIT: f64 = 60.0;
const BOOST_BONUS_SPEED: f64 = 0.1;
const AIM_TIME_SCALE: f64 = 0.1;

/// Marker component indicating a player spawn point, must be attached to a Pose.
///
/// For now, we just find the first one and spawn the player on it.
/// Eventually these will work as checkpoints.
#[derive(Clone, Copy, Debug)]
pub struct PlayerSpawnPoint;

#[derive(Clone, Copy, Debug)]
struct PlayerNodes {
    pose: NodeKey<m::Pose>,
    body: NodeKey<phys::Body>,
    coll: NodeKey<phys::Collider>,
}

#[derive(Clone, Copy, Debug)]
struct AttachedVine {
    rope: rope::RopeProperties,
    player_constraint: phys::ConstraintHandle,
}

/// Whatever the mouse / gamepad aim stick is currently pointing at
/// and whether or not a vine can be created there. Used to draw aiming HUD.
#[derive(Clone, Copy, Debug)]
struct AimTarget {
    point: m::Vec2,
    validity: AimTargetValidity,
}

#[derive(Clone, Copy, Debug)]
enum AimTargetValidity {
    TooClose,
    Valid { collider: NodeKey<phys::Collider> },
    TooFar,
}

/// Controller that holds most of the player's state and handles its actions.
#[derive(Clone, Copy, Debug)]
pub struct PlayerController {
    body: Option<PlayerNodes>,
    attached_vine: Option<AttachedVine>,
    // whether or not slow-down-time-and-show-cool-reticle aiming mode is active
    is_aim_active: bool,
    // aim target is checked even if not in aim mode to draw a simplified indicator
    aim_target: AimTarget,
}

sf::graph::named_layer_bundle! {
    pub struct SpawnLayers<'a> {
        pose: w m::Pose,
        collider: w phys::Collider,
        body: w phys::Body,
        mesh: w gx::Mesh,
        spawn: r PlayerSpawnPoint,
    }
}

sf::graph::named_layer_bundle! {
    pub struct TickLayers<'a> {
        pose: w m::Pose,
        collider: w phys::Collider,
        body: w phys::Body,
        mesh: w gx::Mesh,
        rope: w rope::Rope,
        flammable: w Flammable,
    }
}

impl PlayerController {
    pub fn new() -> Self {
        Self {
            body: None,
            attached_vine: None,
            is_aim_active: false,
            // meaningless default that will be overwritten come first tick,
            // just making validity such that it won't be drawn
            aim_target: AimTarget {
                point: m::Vec2::zero(),
                validity: AimTargetValidity::TooFar,
            },
        }
    }

    pub fn time_scale(&self) -> Option<f64> {
        if self.is_aim_active {
            Some(AIM_TIME_SCALE)
        } else {
            None
        }
    }

    pub fn respawn(&mut self, graph: &mut Graph) {
        if let Some(nodes) = &self.body {
            graph.gather(nodes.body).delete();
        }

        let mut l: SpawnLayers = graph.get_layer_bundle();

        let spawn_point: m::Vec2 = match l
            .spawn
            .iter()
            .next()
            .and_then(|s| s.get_neighbor_mut(&mut l.pose))
        {
            Some(spawn) => spawn.c.translation,
            None => m::Vec2::zero(),
        };

        let mut pose = l
            .pose
            .insert(m::Pose::new(spawn_point, m::Angle::Deg(90.0).into()));
        let mut coll = l.collider.insert(
            phys::Collider::new_capsule(COLL_LENGTH, COLL_R)
                .with_material(phys::Material {
                    static_friction_coef: None,
                    dynamic_friction_coef: None,
                    restitution_coef: 0.0,
                })
                .with_layer(super::collision_layers::PLAYER),
        );
        let mut mesh = l
            .mesh
            .insert(gx::Mesh::from(*coll.c).with_color([0.2, 0.8, 0.6, 1.0]));
        let mut body = l.body.insert(phys::Body::new_particle(PLAYER_MASS));
        pose.connect(&mut body);
        pose.connect(&mut coll);
        body.connect(&mut coll);
        pose.connect(&mut mesh);

        self.body = Some(PlayerNodes {
            pose: pose.key(),
            body: body.key(),
            coll: coll.key(),
        });
    }

    pub fn tick(
        &mut self,
        input: &sf::InputCache,
        cursor_world_pos: m::Vec2,
        keys: &crate::settings::PlayerKeys,
        physics: &mut phys::Physics,
        graph: &mut Graph,
    ) -> Option<()> {
        let nodes = self.body?;

        let mut l: TickLayers = graph.get_layer_bundle();

        // check if attached vine still exists
        // (it may burn even when holding on to it.
        // TODO: once I have some proper level design in place, try only making it flammable when shooting
        // out the other end of it, see if it feels better like that)
        if let Some(attached) = self.attached_vine {
            if l.body.get(attached.rope.last_particle).is_none() {
                self.attached_vine = None;
            }
        }

        let player_body = l.body.get_mut(nodes.body)?.c;
        let player_pose = l.pose.get(nodes.pose)?.c;

        //
        // handle contacts (groundedness, interactables)
        //

        let normal_y_limit = m::Angle::Deg(GROUNDED_ANGLE_LIMIT).rad().cos();

        let most_downright_contact = {
            let mut lowest_cont_y = f64::MAX;
            let mut lowest_cont = None;
            for contact in physics.contacts_for_collider(nodes.coll) {
                let other_coll = match l.collider.get(contact.colliders[1]) {
                    Some(coll) => coll,
                    None => continue,
                };
                if contact.normal.y < lowest_cont_y && other_coll.c.is_solid() {
                    lowest_cont_y = contact.normal.y;
                    lowest_cont = Some(contact);
                }
            }
            lowest_cont
        };

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
        // controls
        //

        // set aim mode if holding shoot button for long enough,
        // exit aim mode if releasing shoot or pressing cancel
        // (and don't aim again until pressing shoot again)

        // TODO: accessibility: also allow click to activate and click again to shoot
        if input.button(ButtonQuery::mouse(keys.shoot).held_exact(keys.aim_delay)) {
            self.is_aim_active = true;
        }
        if input.button(keys.cancel_aim.into())
            || input.button(ButtonQuery::mouse(keys.shoot).released())
        {
            self.is_aim_active = false;
        }

        //
        // move
        //

        let target_hdir = input.axis(AxisQuery {
            pos_btn: keys.right.into(),
            neg_btn: keys.left.into(),
        });
        let target_vdir = input.axis(AxisQuery {
            pos_btn: keys.up.into(),
            neg_btn: keys.down.into(),
        });

        match (groundedness, self.attached_vine) {
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

        if input.button(keys.jump.into()) {
            if let Groundedness::EvenGround(normal) = groundedness {
                player_body.velocity.linear -= JUMP_VEL * *normal;
            }
        } else if input.button(ButtonQuery::from(keys.jump).released())
            && player_body.velocity.linear.y > 0.0
        {
            player_body.velocity.linear.y /= 2.0;
        }

        //
        // aim with mouse
        //

        let player_to_cursor = cursor_world_pos - player_pose.translation;
        let ray_dir = m::Unit::new_normalize(player_to_cursor);
        let ray = phys::Ray {
            start: player_pose.translation,
            dir: ray_dir,
        };
        match physics.spherecast(
            SPHERECAST_RADIUS,
            ray,
            ROPE_MAX_LENGTH,
            (l.pose.subview(), l.collider.subview()),
        ) {
            Some(hit) => {
                self.aim_target = AimTarget {
                    point: ray.point_at_t(hit.t),
                    validity: if hit.t < ROPE_MIN_LENGTH {
                        AimTargetValidity::TooClose
                    } else {
                        AimTargetValidity::Valid {
                            collider: hit.collider,
                        }
                    },
                };
            }
            None => {
                self.aim_target = AimTarget {
                    point: ray.point_at_t(ROPE_MAX_LENGTH),
                    validity: AimTargetValidity::TooFar,
                };
            }
        }

        //
        // shoot vines
        //

        if let (
            true,
            AimTargetValidity::Valid {
                collider: target_collider,
            },
        ) = (
            input.button(ButtonQuery::mouse(keys.shoot).released()),
            self.aim_target.validity,
        ) {
            match self.attached_vine {
                //
                // new vine
                //
                None => {
                    let player_pos = player_pose.translation;
                    // start at the other end to control angle constraint propagation
                    let rope_start = self.aim_target.point;
                    let rope_end = ray.point_at_t(ROPE_START_OFFSET);
                    let rope = rope::spawn_line(
                        rope::Rope {
                            bending_max_angle: m::Angle::Deg(75.0).rad(),
                            bending_compliance: 0.05,
                            ..Default::default()
                        },
                        rope_start,
                        rope_end,
                        (
                            l.body.subview_mut(),
                            l.pose.subview_mut(),
                            l.collider.subview_mut(),
                            l.rope.subview_mut(),
                            l.mesh.subview_mut(),
                        ),
                    );
                    // make it flammable
                    let rope_node = l.rope.get(rope.rope_node).unwrap();
                    let mut iter = rope_node.get_all_neighbors_mut(&mut l.body);
                    while let Some(mut particle) = iter.next() {
                        let mut coll = particle.get_neighbor_mut(&mut l.collider)?;
                        let mut flammable = l.flammable.insert(Flammable::default());
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

                    let coll_hit = l.collider.get_unchecked(target_collider);
                    match coll_hit.get_neighbor(&l.body.subview()) {
                        Some(body) => {
                            let b_pose = body
                                .get_neighbor(&l.pose.subview())
                                .map(|p| *p.c)
                                .unwrap_or_default();
                            let offset = b_pose.inversed() * rope_start;
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

                    self.attached_vine = Some(AttachedVine {
                        rope,
                        player_constraint,
                    });

                    // adjust player velocity towards the circle around the attachment point
                    // for juicy swings

                    // reborrow needed to allow subviews of the layer above
                    let player_body = l.body.get_mut_unchecked(nodes.body).c;

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
                //
                // disconnect existing vine from self and connect to whatever was hit
                //
                Some(attached) => {
                    physics.remove_constraint(attached.player_constraint);
                    self.attached_vine = None;

                    let curr_end_body = l.body.get(attached.rope.last_particle)?;
                    let curr_end = curr_end_body.get_neighbor(&l.pose.subview())?.c.translation;
                    let new_segment_end = self.aim_target.point;
                    let dir = m::Unit::new_normalize(new_segment_end - curr_end);

                    let mut rope_node = l.rope.get_mut(attached.rope.rope_node)?;
                    let dist = (new_segment_end - curr_end).mag();
                    let new_particle_count = (dist / rope_node.c.spacing) as usize;

                    let new_rope = rope::extend_line(
                        &mut rope_node,
                        dir,
                        new_particle_count,
                        (
                            l.body.subview_mut(),
                            l.pose.subview_mut(),
                            l.collider.subview_mut(),
                            l.mesh.subview_mut(),
                        ),
                    );
                    // make the newly added part flammable
                    let rope_node = l.rope.get(new_rope.rope_node).unwrap();
                    let mut iter = rope_node.get_all_neighbors_mut(&mut l.body);
                    while let Some(mut particle) = iter.next() {
                        if particle.get_neighbor_mut(&mut l.flammable).is_none() {
                            let mut coll = particle.get_neighbor_mut(&mut l.collider)?;
                            let mut flammable = l.flammable.insert(Flammable::default());
                            flammable.connect(&mut coll);
                            flammable.connect(&mut particle);
                        }
                    }

                    // constraint on the new target

                    let coll_hit = l.collider.get_unchecked(target_collider);
                    match coll_hit.get_neighbor(&l.body.subview()) {
                        Some(body) => {
                            let offset = new_segment_end
                                - body
                                    .get_neighbor(&l.pose.subview())
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

        //
        // remove held vine
        //
        if !self.is_aim_active && input.button(keys.retract_vine.into()) {
            if let Some(attached) = self.attached_vine {
                self.attached_vine = None;
                // in the future, maybe "pull in" the vine particle by particle
                // for a nice animation. for now, just delete it
                drop(l);
                graph.gather(attached.rope.rope_node).delete();
            }
        }

        Some(())
    }
}
