pub(super) fn health(round: u32) -> i32 {
    let mut health = 150i32;
    for number in 2..=round.min(255) {
        let increment = if number < 10 {
            100
        } else {
            (health as f32 * 0.1) as i32
        };
        let Some(next) = health.checked_add(increment) else {
            break;
        };
        health = next;
    }
    health
}

pub(super) fn population(round: u32, players: usize) -> u32 {
    let round = round.clamp(1, 255);
    let players = players.max(1);
    let mut multiplier = (round as f32 / 5.0).max(1.0);
    if round >= 10 {
        multiplier *= round as f32 * 0.15;
    }
    let extra = if players == 1 {
        3.0
    } else {
        (players - 1) as f32 * 6.0
    };
    let maximum = 24u32.saturating_add((extra * multiplier) as u32);
    let fraction = match round {
        1 => 0.25,
        2 => 0.3,
        3 => 0.5,
        4 => 0.7,
        5 => 0.9,
        _ => 1.0,
    };
    (maximum as f32 * fraction) as u32
}

pub(super) fn spawn_delay_ms(round: u32) -> u32 {
    let mut delay = 2.0f32;
    for _ in 1..round.min(255) {
        if delay > 0.08 {
            delay *= 0.95;
        } else if delay < 0.08 {
            delay = 0.08;
        }
    }
    (delay * 1000.0).ceil() as u32
}
