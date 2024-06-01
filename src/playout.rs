use std::time::Duration;

use crate::{board_tracker::BoardTracker, Strategy};

pub struct PlayoutResult {
    pub beaten_players: usize,
    pub remaining_players: usize,
    pub survived_steps: usize,
    pub did_win: bool,
    pub did_die: bool,
}

pub fn run_playout(
    mut board: BoardTracker,
    mut strategies_by_player: Vec<Box<dyn Strategy>>,
    own_player_id: usize,
    max_steps: usize,
    clear_on_death: bool,
) -> PlayoutResult {
    assert!(!board.is_dead(own_player_id));
    assert!(max_steps > 0);

    let (width, _height) = board.board_size();

    for i_step in 0.. {
        let count_dead_before_turn = board.count_dead();

        let new_pos_by_player: Vec<Option<(usize, usize)>> = strategies_by_player
            .iter_mut()
            .enumerate()
            .map(|(player_id, strategy)| {
                if board.is_dead(player_id) {
                    None
                } else {
                    let direction = strategy.step(&board, Duration::from_secs(0));
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
                    board.record_death(player_id, clear_on_death);
                }
            }
        }

        if board.is_dead(own_player_id) {
            return PlayoutResult {
                beaten_players: count_dead_before_turn,
                remaining_players: board.count_alive(),
                survived_steps: i_step,
                did_win: false,
                did_die: true,
            };
        } else if board.count_alive() == 1 {
            return PlayoutResult {
                beaten_players: board.count_dead(),
                remaining_players: board.count_alive(),
                survived_steps: i_step + 1,
                did_win: true,
                did_die: false,
            };
        } else if i_step + 1 >= max_steps {
            assert!(i_step + 1 == max_steps);
            return PlayoutResult {
                beaten_players: board.count_dead(),
                remaining_players: board.count_alive(),
                survived_steps: i_step + 1,
                did_win: false,
                did_die: false,
            };
        }
    }

    panic!("step overflow")
}
