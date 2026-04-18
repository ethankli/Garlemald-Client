mod game_launch;
mod pe_patch;

pub use game_launch::{launch_game, GameLaunchRequest, PatchSpec};
pub use pe_patch::{
    apply_patches_on_disk, encryption_time_patch, lobby_host_patch, PePatch,
    ENCRYPTION_TIME_PATCH_RVA, LOBBY_HOST_NAME_RVA, LOBBY_HOST_NAME_SLOT_SIZE,
};
