//! Player controller

use assets_manager::asset::Gltf;

use starframe as sf;

use crate::{fire::Flammable, AssetHandle, ASSETS};

// tuning constants

const COLL_R: f64 = 0.3;
const COLL_LENGTH: f64 = 1.2;
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
struct AttachedVine {
    rope_key: sf::RopeKey,
    player_constraint: sf::ConstraintKey,
}

/// Whatever the mouse / gamepad aim stick is currently pointing at
/// and whether or not a vine can be created there. Used to draw aiming HUD.
#[derive(Clone, Copy, Debug)]
struct AimTarget {
    point: sf::Vec2,
    validity: AimTargetValidity,
}

#[derive(Clone, Copy, Debug)]
enum AimTargetValidity {
    TooClose,
    Valid { collider: sf::ColliderKey },
    TooFar,
}

/// Controller that holds most of the player's state and handles its actions.
#[derive(Clone, Copy, Debug)]
pub struct PlayerController {
    entity: Option<sf::hecs::Entity>,
    attached_vine: Option<AttachedVine>,
    // whether or not slow-down-time-and-show-cool-reticle aiming mode is active
    is_aim_active: bool,
    // aim target is checked even if not in aim mode to draw a simplified indicator
    aim_target: AimTarget,
    mesh: AssetHandle<Gltf>,
}

impl PlayerController {
    pub fn new() -> Self {
        Self {
            entity: None,
            attached_vine: None,
            is_aim_active: false,
            // meaningless default that will be overwritten come first tick,
            // just making validity such that it won't be drawn
            aim_target: AimTarget {
                point: sf::Vec2::zero(),
                validity: AimTargetValidity::TooFar,
            },
            mesh: ASSETS
                .load::<Gltf>("models.player")
                .expect("Missing or invalid player model"),
        }
    }

    pub fn time_scale(&self) -> Option<f64> {
        if self.is_aim_active {
            Some(AIM_TIME_SCALE)
        } else {
            None
        }
    }

    pub fn respawn(&mut self, physics: &mut sf::PhysicsWorld, world: &mut sf::hecs::World) {
        if let Some(entity) = self.entity {
            world.despawn(entity).ok();
        }

        let spawn_point: sf::Vec2 = match world
            .query_mut::<(&sf::Pose, &PlayerSpawnPoint)>()
            .into_iter()
            .next()
        {
            Some((_, (spawn, _))) => spawn.translation,
            None => sf::Vec2::zero(),
        };

        let pose = sf::Pose::new(spawn_point, sf::Angle::Deg(90.0).into());
        let body = sf::Body::new_particle(PLAYER_MASS);
        let body_key = physics.entity_set.insert_body(body);
        let coll = sf::Collider::new_capsule(COLL_LENGTH, COLL_R)
            .with_material(sf::PhysicsMaterial {
                static_friction_coef: None,
                dynamic_friction_coef: None,
                restitution_coef: 0.0,
            })
            .with_layer(super::collision_layers::PLAYER);
        let coll_key = physics.entity_set.attach_collider(body_key, coll);

        let mesh_gltf = self.mesh.read();
        let mesh_bufs: Vec<&[u8]> = mesh_gltf
            .document
            .buffers()
            .map(|b| mesh_gltf.get_buffer(&b))
            .collect();
        let mesh = sf::Mesh::from_gltf(&mesh_gltf.document, &mesh_bufs).with_offset(sf::Pose::new(
            sf::Vec2::new(-COLL_LENGTH / 2.0 - COLL_R, 0.0),
            sf::Angle::Deg(-90.0).into(),
        ));
        let skin = sf::gltf_import::load_skin(&mesh_gltf.document, &mesh_bufs)
            .expect("no skin in player gltf");
        let mut anim = sf::gltf_import::load_animations(&mesh_gltf.document, &mesh_bufs)
            .expect("no skin in player gltf");
        anim.activate_animation("walk").unwrap();

        self.entity = Some(world.spawn((pose, body_key, coll_key, mesh, skin, anim)));
    }

    pub fn tick(
        &mut self,
        input: &sf::Input,
        camera: &mut sf::Camera,
        keys: &crate::settings::PlayerKeys,
        physics: &mut sf::PhysicsWorld,
        world: &mut sf::hecs::World,
    ) -> Option<()> {
        let entity = self.entity?;
        let (player_pose, &player_body_key, &player_coll_key) = world
            .query_one_mut::<(&mut sf::Pose, &sf::BodyKey, &sf::ColliderKey)>(entity)
            .ok()?;

        // check if attached vine still exists
        // (it may burn even when holding on to it.
        // TODO: once I have some proper level design in place, try only making it flammable when shooting
        // out the other end of it, see if it feels better like that)
        if let Some(attached) = self.attached_vine {
            if physics
                .constraint_set
                .get(attached.player_constraint)
                .is_none()
            {
                self.attached_vine = None;
            }
        }

        // hacking in camera following the player like this for now,
        // TODO: make it smooth
        camera.transform.translation = player_pose.translation;

        //
        // handle contacts (groundedness, interactables)
        //

        let normal_y_limit = sf::Angle::Deg(GROUNDED_ANGLE_LIMIT).rad().cos();

        let most_downright_contact = {
            let mut lowest_cont_y = f64::MAX;
            let mut lowest_cont = None;
            for contact in physics.contacts_for_collider(player_coll_key) {
                let Some(other_coll) = physics.entity_set.get_collider(contact.colliders[1]) else { continue };
                if contact.normal.y < lowest_cont_y && other_coll.is_solid() {
                    lowest_cont_y = contact.normal.y;
                    lowest_cont = Some(contact);
                }
            }
            lowest_cont
        };

        #[derive(Debug, Clone, Copy)]
        enum Groundedness {
            EvenGround(sf::Unit<sf::Vec2>),
            SteepSlope(sf::Unit<sf::Vec2>),
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
        if input.button(sf::ButtonQuery::mouse(keys.shoot).held_exact(keys.aim_delay)) {
            self.is_aim_active = true;
        }
        if input.button(keys.cancel_aim.into())
            || input.button(sf::ButtonQuery::mouse(keys.shoot).released())
        {
            self.is_aim_active = false;
        }

        //
        // move
        //

        {
            let player_body = physics.entity_set.get_body_mut(player_body_key)?;

            let target_hdir = input.axis(sf::AxisQuery {
                pos_btn: keys.right.into(),
                neg_btn: keys.left.into(),
            });
            let target_vdir = input.axis(sf::AxisQuery {
                pos_btn: keys.up.into(),
                neg_btn: keys.down.into(),
            });
            match (groundedness, self.attached_vine) {
                // special acceleration-based controls for in air with a rope
                // for improved swing feel and control, hopefully
                (Groundedness::Air, Some(_rope)) => {
                    if target_hdir != 0.0 || target_vdir != 0.0 {
                        let target_dir =
                            sf::Unit::new_normalize(sf::Vec2::new(target_hdir, target_vdir));
                        player_body.velocity.linear += ROPE_SWINGING_ACCEL * *target_dir;
                    }
                }
                // normal controls for all other situations
                _ => {
                    let ground_dir = match groundedness {
                        // on ground, match ground slope
                        Groundedness::EvenGround(normal) => sf::math::left_normal(*normal),
                        // in air or on a steep slope, normal horizontal air acceleration
                        _ => sf::Vec2::unit_x(),
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
            } else if input.button(sf::ButtonQuery::from(keys.jump).released())
                && player_body.velocity.linear.y > 0.0
            {
                player_body.velocity.linear.y /= 2.0;
            }
        }

        //
        // aim with mouse
        //

        let player_to_cursor = input.cursor_position_world(camera) - player_pose.translation;
        let ray_dir = sf::Unit::new_normalize(player_to_cursor);
        let ray = sf::Ray {
            start: player_pose.translation,
            dir: ray_dir,
        };
        match physics.spherecast(SPHERECAST_RADIUS, ray, ROPE_MAX_LENGTH) {
            Some(hit) => {
                self.aim_target = AimTarget {
                    point: ray.point_at_t(hit.t),
                    validity: if self.attached_vine.is_none() && hit.t < ROPE_MIN_LENGTH {
                        // can't create a new vine super close.
                        // if holding onto a vine, you can attach that to something
                        // even if it's right under your feet
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
            input.button(sf::ButtonQuery::mouse(keys.shoot).released()),
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
                    let rope = sf::Rope::spawn_line(
                        sf::RopeParameters {
                            bending_max_angle: sf::Angle::Deg(75.0).rad(),
                            bending_compliance: 0.05,
                            ..Default::default()
                        },
                        rope_start,
                        rope_end,
                        &mut physics.entity_set,
                    );
                    // make it flammable and add visuals to the particles
                    for &particle in &rope.particles {
                        let mesh = sf::Mesh::from(sf::ConvexMeshShape::Circle {
                            r: rope.params.thickness / 2.0,
                            points: 8,
                        })
                        .with_color([0.729, 0.855, 0.333, 1.0]);
                        world.spawn((
                            physics.entity_set.get_body(particle.body).unwrap().pose,
                            particle.body,
                            particle.collider,
                            mesh,
                            Flammable::default(),
                        ));
                    }

                    // constraint on the player

                    let player_constraint = physics.constraint_set.insert(
                        sf::ConstraintBuilder::new(player_body_key)
                            .with_target(
                                rope.particles
                                    .iter()
                                    .last()
                                    .expect("Rope had no particles")
                                    .body,
                            )
                            .with_limit(sf::ConstraintLimit::Lt)
                            .build_distance((rope_end - player_pos).mag()),
                    );

                    // constraint on the target

                    match physics.entity_set.get_collider_body_key(target_collider) {
                        Some(body) => {
                            let offset = physics.entity_set.get_body(body).unwrap().pose.inversed()
                                * rope_start;
                            physics.constraint_set.insert(
                                sf::ConstraintBuilder::new(
                                    rope.particles.first().expect("Rope had no particles").body,
                                )
                                .with_target(body)
                                .with_target_origin(offset)
                                .build_attachment(),
                            );
                        }
                        None => {
                            physics.constraint_set.insert(
                                sf::ConstraintBuilder::new(
                                    rope.particles.first().expect("Rope had no particles").body,
                                )
                                .with_target_origin(rope_start)
                                .build_attachment(),
                            );
                        }
                    }

                    let rope_key = physics.rope_set.insert(rope);
                    self.attached_vine = Some(AttachedVine {
                        rope_key,
                        player_constraint,
                    });

                    // adjust player velocity towards the circle around the attachment point
                    // for juicy swings

                    let player_body = physics.entity_set.get_body_mut(player_body_key).unwrap();
                    let vel_mag = player_body.velocity.linear.mag();
                    let vel_dir = sf::Unit::new_unchecked(player_body.velocity.linear / vel_mag);

                    let player_to_center = rope_start - player_pos;
                    let tangent = sf::Unit::new_normalize(sf::math::left_normal(player_to_center));
                    let tan_dot_vel = tangent.dot(*vel_dir);
                    let (tangent, tan_dot_vel) = if tan_dot_vel >= 0.0 {
                        (tangent, tan_dot_vel)
                    } else {
                        (-tangent, -tan_dot_vel)
                    };

                    let dot_limit = sf::Angle::Deg(BOOST_ANGLE_LIMIT).rad().cos();
                    if tan_dot_vel > dot_limit {
                        player_body.velocity.linear = (BOOST_BONUS_SPEED + vel_mag) * *tangent;
                    }
                }
                //
                // disconnect existing vine from self and connect to whatever was hit
                //
                Some(attached) => {
                    physics.constraint_set.remove(attached.player_constraint);
                    self.attached_vine = None;

                    let rope = physics.rope_set.get_mut(attached.rope_key)?;
                    let curr_end_body = physics
                        .entity_set
                        .get_body(rope.particles.iter().last().unwrap().body)?;
                    let curr_end = curr_end_body.pose.translation;
                    let new_segment_end = self.aim_target.point;
                    let dir = sf::Unit::new_normalize(new_segment_end - curr_end);

                    let dist = (new_segment_end - curr_end).mag();
                    let new_particle_count = (dist / rope.params.spacing) as usize;

                    let old_particle_count = rope.particles.len();
                    rope.extend_line(dir, new_particle_count, &mut physics.entity_set);

                    // make the newly added part flammable and add visuals
                    for &particle in rope.particles.iter().skip(old_particle_count) {
                        let mesh = sf::Mesh::from(sf::ConvexMeshShape::Circle {
                            r: rope.params.thickness / 2.0,
                            points: 8,
                        })
                        .with_color([0.729, 0.855, 0.333, 1.0]);
                        world.spawn((
                            physics.entity_set.get_body(particle.body).unwrap().pose,
                            particle.body,
                            particle.collider,
                            mesh,
                            Flammable::default(),
                        ));
                    }

                    // constraint on the new target

                    match physics.entity_set.get_collider_body_key(target_collider) {
                        Some(body_key) => {
                            let body = physics.entity_set.get_body_mut(body_key).unwrap();
                            let offset = new_segment_end - body.pose.translation;
                            physics.constraint_set.insert(
                                sf::ConstraintBuilder::new(
                                    rope.particles.iter().last().unwrap().body,
                                )
                                .with_target(body_key)
                                .with_target_origin(offset)
                                .build_attachment(),
                            );
                        }
                        None => {
                            physics.constraint_set.insert(
                                sf::ConstraintBuilder::new(
                                    rope.particles.iter().last().unwrap().body,
                                )
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
                physics
                    .rope_set
                    .remove(attached.rope_key, &mut physics.entity_set);
            }
        }

        Some(())
    }
}
