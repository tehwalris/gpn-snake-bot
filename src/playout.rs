use crate::board_tracker::BoardTracker;

struct PlayoutResult {
    beaten_players: usize,
    did_win: bool,
}

fn run_playout(board: BoardTracker, strategies_by_player: Vec<Box<dyn Strategy>>) -> PlayoutResult {
}
