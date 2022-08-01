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

pub fn tick(dt: f64, physics: &mut sf::Physics, graph: &mut sf::Graph) {
    let mut l_flammable = graph.get_layer_mut::<Flammable>();
    let l_collider = graph.get_layer::<sf::Collider>();
    let l_pose = graph.get_layer::<sf::Pose>();
    let mut l_body = graph.get_layer_mut::<sf::Body>();
    let mut l_rope = graph.get_layer_mut::<sf::Rope>();

    // reset cooling down state

    for flammable in l_flammable.iter_mut() {
        if let FlammableState::NotOnFire {
            ref mut cooling_down,
            ..
        } = flammable.c.state
        {
            *cooling_down = true;
        }
    }

    // heat up adjacent flammables

    // defer mutation to dodge lifetime shenanigans
    let mut delta_temps: Vec<(sf::NodeKey<Flammable>, f64)> = Vec::new();
    let l_flammable_immut = l_flammable.subview();
    for flammable in l_flammable_immut
        .iter()
        .filter(|f| matches!(f.c.state, FlammableState::OnFire { .. }))
    {
        let coll = match flammable.get_neighbor(&l_collider) {
            Some(coll) => coll,
            None => continue,
        };
        let pose = match coll.get_neighbor(&l_pose) {
            Some(pose) => pose,
            None => continue,
        };
        for other_coll in physics.query_shape(
            *pose.c,
            coll.c.shape.expanded(FIRE_SPREAD_RANGE),
            Default::default(),
            &(l_pose.subview(), l_collider.subview()),
        ) {
            if other_coll.key() == coll.key() {
                continue;
            }
            let other_flammable = match other_coll.get_neighbor(&l_flammable_immut) {
                Some(f) => f,
                None => continue,
            };
            delta_temps.push((other_flammable.key(), flammable.c.params.burning_heat * dt));
        }
    }
    drop(l_flammable_immut);
    for (key, delta_temp) in delta_temps {
        let flammable = l_flammable.get_mut_unchecked(key);
        if let FlammableState::NotOnFire {
            temperature,
            cooling_down,
        } = &mut flammable.c.state
        {
            *temperature += delta_temp;
            *cooling_down = false;
        }
    }

    // cool down ones that weren't heated up,
    // ignite ones that heated up enough,
    // destroy ones that burned for long enough

    let mut to_destroy: Vec<sf::NodeKey<Flammable>> = Vec::new();
    for flammable in l_flammable.iter_mut() {
        match flammable.c.state {
            FlammableState::OnFire {
                ref mut time_burning,
            } => {
                *time_burning += dt;
                if *time_burning >= flammable.c.params.time_to_destroy.unwrap_or(f64::INFINITY) {
                    to_destroy.push(flammable.key());
                    // if this is a rope particle, disconnect it from the rope
                    // so the whole thing doesn't get deleted at once
                    let body = match flammable.get_neighbor_mut(&mut l_body) {
                        Some(b) => b,
                        None => continue,
                    };
                    let _rope = match body.get_neighbor_mut(&mut l_rope) {
                        Some(r) => r,
                        None => continue,
                    };
                    sf::rope::cut_after(body.key(), (l_body.subview_mut(), l_rope.subview_mut()));
                }
            }
            FlammableState::NotOnFire {
                ref mut temperature,
                cooling_down,
            } => {
                if cooling_down {
                    *temperature = (*temperature - flammable.c.params.cooldown_rate * dt).max(0.0);
                } else if *temperature >= flammable.c.params.temp_to_catch_fire {
                    flammable.c.ignite();
                }
            }
        }
    }

    drop((l_flammable, l_collider, l_pose, l_body, l_rope));

    for flammable in to_destroy {
        graph.gather(flammable).stop_at_layer::<sf::Rope>().delete();
    }
}
