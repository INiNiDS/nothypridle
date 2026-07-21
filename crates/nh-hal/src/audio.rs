use mpris::{PlaybackStatus, PlayerFinder};
use tracing::{error, warn};

pub fn is_any_audio_playing() -> bool {
    let finder = match PlayerFinder::new() {
        Ok(finder) => finder,
        Err(err) => {
            error!("MPRIS Finder cannot start: {}", err);
            return false;
        }
    };

    let players = match finder.iter_players() {
        Ok(players) => players,
        Err(err) => {
            warn!("Failed to find any MPRIS players: {}", err);
            return false;
        }
    };

    for player in players.flatten() {
        if !player.is_running() {
            continue;
        }
        if let Ok(status) = player.get_playback_status()
            && status == PlaybackStatus::Playing
        {
            return true;
        }
    }

    false
}

pub fn is_audio_playing(name: &str) -> bool {
    let finder = match PlayerFinder::new() {
        Ok(finder) => finder,
        Err(err) => {
            error!("MPRIS Finder cannot start: {}", err);
            return false;
        }
    };

    let players = match finder.iter_players() {
        Ok(players) => players,
        Err(err) => {
            warn!("Failed to find any MPRIS players: {}", err);
            return false;
        }
    };

    let name_lower = name.to_lowercase();
    for player in players.flatten() {
        if !player.is_running() {
            continue;
        }
        if player.identity().to_lowercase() == name_lower
            && let Ok(status) = player.get_playback_status()
            && status == PlaybackStatus::Playing
        {
            return true;
        }
    }

    false
}
