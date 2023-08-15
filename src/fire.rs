//! Logic for propagating fire and having it destroy things.

use starframe as sf;

const FIRE_SPREAD_RANGE: f64 = 0.2;

/// Component that marks things as able to catch fire.
#[derive(Clone, Copy, Debug)]
pub struct Flammable {
    params: FlammableParams,
    state: FlammableState,
}
impl Default for Flammable {
    fn default() -> Self {
        Self::new(Default::default())
    }
}
impl Flammable {
    #[inline]
    pub fn new(params: FlammableParams) -> Self {
        Self {
            params,
            state: FlammableState::default(),
        }
    }

    #[inline]
    pub fn ignite(&mut self) {
        self.state = FlammableState::OnFire { time_burning: 0.0 };
    }

    #[inline]
    pub fn ignited(mut self) -> Self {
        self.ignite();
        self
    }
}

#[derive(Clone, Copy, Debug)]
enum FlammableState {
    NotOnFire {
        // acts as a sort of timer to ignite when adjacent things are on fire
        temperature: f64,
        cooling_down: bool,
    },
    OnFire {
        time_burning: f64,
    },
}
impl Default for FlammableState {
    fn default() -> Self {
        Self::NotOnFire {
            temperature: 0.0,
            cooling_down: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FlammableParams {
    pub temp_to_catch_fire: f64,
    /// Time the object spends on fire before being destroyed.
    /// Set to None to burn forever
    pub time_to_destroy: Option<f64>,
    /// Temperature of adjacent flammable things increases by this per second
    pub burning_heat: f64,
    /// Temperature of this decreases by this per second if nothing is burning nearby
    pub cooldown_rate: f64,
}
impl Default for FlammableParams {
    fn default() -> Self {
        Self {
            temp_to_catch_fire: 10.0,
            time_to_destroy: Some(0.066),
            burning_heat: 300.0,
            cooldown_rate: 2.0,
        }
    }
}

//
// tick
//

pub fn tick(
    dt: f64,
    physics: &mut sf::PhysicsWorld,
    world: &mut sf::hecs::World,
    hecs_sync: &mut sf::HecsSyncManager,
) {
    // reset cooling down state

    for (_, flammable) in world.query_mut::<&mut Flammable>() {
        if let FlammableState::NotOnFire {
            ref mut cooling_down,
            ..
        } = flammable.state
        {
            *cooling_down = true;
        }
    }

    // heat up adjacent flammables

    // defer mutation to avoid nested mutable hecs queries
    let mut delta_temps: Vec<(sf::hecs::Entity, f64)> = Vec::new();
    for (_, (flammable, &coll_key, pose)) in
        world.query_mut::<(&Flammable, &sf::ColliderKey, &sf::Pose)>()
    {
        let FlammableState::OnFire { .. } = flammable.state else { continue };
        let Some(coll) = physics.entity_set.get_collider(coll_key) else { continue };

        for (other_coll_key, _) in physics.query_shape(
            *pose,
            coll.shape.expanded(FIRE_SPREAD_RANGE),
            Default::default(),
        ) {
            if other_coll_key == coll_key {
                continue;
            }
            let Some(other_entity) = hecs_sync.get_collider_entity(other_coll_key) else { continue };
            // not checking if the other entity has a Flammable component here,
            // we'll need to query for it in the next loop anyway so we can do the check there
            delta_temps.push((other_entity, flammable.params.burning_heat * dt));
        }
    }
    for (entity, delta_temp) in delta_temps {
        let Ok(flammable) = world.query_one_mut::<&mut Flammable>(entity) else { continue };
        if let FlammableState::NotOnFire {
            temperature,
            cooling_down,
        } = &mut flammable.state
        {
            *temperature += delta_temp;
            *cooling_down = false;
        }
    }

    // cool down ones that weren't heated up,
    // ignite ones that heated up enough,
    // destroy ones that burned for long enough

    let mut to_destroy: Vec<sf::hecs::Entity> = Vec::new();
    for (entity, flammable) in world.query_mut::<&mut Flammable>() {
        match &mut flammable.state {
            FlammableState::OnFire { time_burning } => {
                *time_burning += dt;
                if *time_burning >= flammable.params.time_to_destroy.unwrap_or(f64::INFINITY) {
                    to_destroy.push(entity);
                }
            }
            FlammableState::NotOnFire {
                temperature,
                cooling_down,
            } => {
                if *cooling_down {
                    *temperature = (*temperature - flammable.params.cooldown_rate * dt).max(0.0);
                } else if *temperature >= flammable.params.temp_to_catch_fire {
                    flammable.ignite();
                }
            }
        }
    }

    for entity in to_destroy {
        world.despawn(entity).ok();
    }
}
