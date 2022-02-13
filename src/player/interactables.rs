//! Pickups and such

pub enum Interactable {
    FireFlower { taken: bool },
}

impl Interactable {
    pub fn on_contact(&mut self, player: &mut super::PlayerController) {
        match self {
            Interactable::FireFlower { taken } => {
                if !*taken {
                    *taken = true;
                    player.has_fire = true;
                }
            }
        }
    }
}
