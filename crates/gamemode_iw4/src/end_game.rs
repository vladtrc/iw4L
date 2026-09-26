pub(crate) const ONLY_ROUND_EXIT_WAIT_MS: i32 = 3_000;

pub(crate) const MULTI_ROUND_EXIT_WAIT_MS: i32 = 6_000;

pub(crate) const POST_GAME_NOTIFY_BASE_MS: i32 = 4_000;

pub(crate) const POST_GAME_NOTIFY_CAP_MS: i32 = 10_000;

pub const FINAL_KILLCAM_POLL_MS: i32 = 50;

pub const fn exit_wait_ms(only_round: bool, post_game_notifies: i32) -> i32 {
    if post_game_notifies <= 0 {
        if only_round {
            ONLY_ROUND_EXIT_WAIT_MS
        } else {
            MULTI_ROUND_EXIT_WAIT_MS
        }
    } else {
        let want = POST_GAME_NOTIFY_BASE_MS + post_game_notifies * 1_000;
        if want < POST_GAME_NOTIFY_CAP_MS {
            want
        } else {
            POST_GAME_NOTIFY_CAP_MS
        }
    }
}
