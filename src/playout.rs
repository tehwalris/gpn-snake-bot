use crate::{board_tracker::BoardTracker, Strategy};

struct PlayoutResult {
    beaten_players: usize,
    did_win: bool,
}

fn run_playout(
    mut board: BoardTracker,
    mut strategies_by_player: Vec<Box<dyn Strategy>>,
    own_player_id: usize,
) -> PlayoutResult {
    assert!(board.is_dead(own_player_id));

    let (width, _height) = board.board_size();

    loop {
        let count_dead_before_turn = board.count_dead();

        let new_pos_by_player: Vec<Option<(usize, usize)>> = strategies_by_player
            .iter_mut()
            .enumerate()
            .map(|(player_id, strategy)| {
                if board.is_dead(player_id) {
                    None
                } else {
                    let direction = strategy.step(&board);
                    let old_pos = board.get_player_latest_pos(player_id).unwrap();
                    let new_pos = board.offset_pos(old_pos, direction);
                    Some(new_pos)
                }
            })
            .collect();

        let mut next_occupied_count: Vec<usize> =
            board.occupied_mask().iter().map(|v| *v as usize).collect();
        for new_pos in new_pos_by_player.iter().flatten() {
            let new_i = new_pos.1 * width + new_pos.0;
            next_occupied_count[new_i] += 1;
        }

        for (player_id, new_pos) in new_pos_by_player.iter().enumerate() {
            if let &Some(new_pos) = new_pos {
                let new_i = new_pos.1 * width + new_pos.0;
                if next_occupied_count[new_i] == 1 {
                    board.record_pos(player_id, new_pos);
                } else {
                    board.record_death(player_id);
                }
            }
        }

        if board.is_dead(own_player_id) {
            return PlayoutResult {
                beaten_players: count_dead_before_turn,
                did_win: false,
            };
        } else if board.count_alive() == 1 {
            return PlayoutResult {
                beaten_players: board.count_dead(),
                did_win: true,
            };
        }
    }
}
