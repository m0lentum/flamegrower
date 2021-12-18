//! Logic for propagating fire and having it destroy things.

use starframe::{
    graph::NodeKey,
    math as m,
    physics::{Collider, Physics},
};

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
    pub time_to_burn: f64,
    /// Temperature of adjacent flammable things increases by this per second
    pub burning_heat: f64,
    /// Temperature of this decreases by this per second if nothing is burning nearby
    pub cooldown_rate: f64,
}
impl Default for FlammableParams {
    fn default() -> Self {
        Self {
            temp_to_catch_fire: 10.0,
            time_to_burn: 0.066,
            burning_heat: 300.0,
            cooldown_rate: 2.0,
        }
    }
}

//
// tick
//

pub fn tick(dt: f64, physics: &mut Physics, graph: &mut super::MyGraph) {
    let mut l_flammable = graph.get_layer_mut::<Flammable>();
    let l_collider = graph.get_layer::<Collider>();
    let l_pose = graph.get_layer::<m::Pose>();

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
    let mut delta_temps: Vec<(NodeKey<Flammable>, f64)> = Vec::new();
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
            delta_temps.push((other_flammable.key(), flammable.c.params.burning_heat));
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

    // cool down ones that weren't heated up
    // and ignite ones that heated up enough

    for flammable in l_flammable.iter_mut() {
        match flammable.c.state {
            FlammableState::OnFire {
                ref mut time_burning,
            } => {
                *time_burning += dt;
                if *time_burning >= flammable.c.params.time_to_burn {
                    // TODO: destroy (somewhat involved because of rope particles)
                }
            }
            FlammableState::NotOnFire {
                ref mut temperature,
                cooling_down,
            } => {
                if cooling_down {
                    *temperature = (*temperature - flammable.c.params.cooldown_rate).max(0.0);
                } else if *temperature >= flammable.c.params.temp_to_catch_fire {
                    flammable.c.ignite();

                    // temporary hackery to test visually that fire spreads
                    // before implementing destroying
                    // TODO: build temperature into rendering as a tint
                    // (and correctly destroy stuff when it burns down)
                    let mut l_shape = graph.get_layer_mut::<starframe::graphics::Shape>();
                    if let Some(shape) = flammable
                        .get_neighbor(&l_collider)
                        .and_then(|coll| coll.get_neighbor(&l_pose))
                        .and_then(|pose| pose.get_neighbor_mut(&mut l_shape))
                    {
                        shape.c.set_color([1.0, 0.2, 0.3, 1.0]);
                    }
                }
            }
        }
    }
}
