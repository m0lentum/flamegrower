use assets_manager::{loader, Asset};
use starframe::input::Key;

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct Settings {
    pub keymap: Keymap,
}

// For now, treating settings as an asset.
// TODO: this should eventually be modifiable in-game
// and written in a standard location.
// Probably figure this out when implementing save files
impl Asset for Settings {
    const EXTENSION: &'static str = "json";

    type Loader = loader::JsonLoader;
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct Keymap {
    pub menus: MenuKeys,
    pub player: PlayerKeys,
    pub debug: DebugKeys,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct MenuKeys {
    pub exit: Key,
    // Temporary key to quickly reload the level.
    // This will happen through menus when we have them (and on player respawn)
    pub reload: Key,
    pub pause: Key,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct PlayerKeys {
    pub right: Key,
    pub left: Key,
    pub up: Key,
    pub down: Key,
    pub jump: Key,
    pub aim_new: Key,
    pub ignite: Key,
    pub aim_connect: Key,
    pub respawn: Key,
}

#[derive(Clone, Copy, Debug, serde::Deserialize, serde::Serialize)]
pub struct DebugKeys {
    pub toggle_grid: Key,
}
