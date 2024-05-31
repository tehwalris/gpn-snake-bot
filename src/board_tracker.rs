use crate::direction::Direction;

#[derive(Clone)]
pub struct BoardTracker {
    width: usize,
    height: usize,
    board: Vec<usize>,
    latest_pos_by_player: Vec<Option<(usize, usize)>>,
}

impl BoardTracker {
    const NO_PLAYER: usize = usize::MAX;

    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            board: vec![Self::NO_PLAYER; width * height],
            latest_pos_by_player: vec![],
        }
    }

    pub fn board_size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn get_cell_player(&self, (x, y): (usize, usize)) -> Option<usize> {
        match self.board[y * self.width + x] {
            Self::NO_PLAYER => None,
            player_id => Some(player_id),
        }
    }

    pub fn get_player_latest_pos(&self, player_id: usize) -> Option<(usize, usize)> {
        match self.latest_pos_by_player.get(player_id) {
            Some(&Some(pos)) => Some(pos),
            _ => None,
        }
    }

    pub fn record_pos(&mut self, player_id: usize, (x, y): (usize, usize)) -> bool {
        let v = &mut self.board[y * self.width + x];
        let duplicate = *v != Self::NO_PLAYER;
        *v = player_id;

        if player_id >= self.latest_pos_by_player.len() {
            self.latest_pos_by_player.resize(player_id + 1, None);
        }
        self.latest_pos_by_player[player_id] = Some((x, y));

        duplicate
    }

    pub fn record_death(&mut self, player_id: usize) {
        if player_id < self.latest_pos_by_player.len() {
            self.latest_pos_by_player[player_id] = None;
        }

        for i in 0..self.board.len() {
            if self.board[i] == player_id {
                self.board[i] = Self::NO_PLAYER;
            }
        }
    }

    pub fn offset_pos(&self, pos: (usize, usize), direction: Direction) -> (usize, usize) {
        direction.offset_pos(pos, (self.width, self.height))
    }

    pub fn occupied_mask(&self) -> Vec<bool> {
        self.board
            .iter()
            .map(|&player_id| player_id != Self::NO_PLAYER)
            .collect()
    }

    pub fn conservative_occupied_mask(&self, own_player: usize) -> Vec<bool> {
        let mut mask = self.occupied_mask();
        for pos in self
            .latest_pos_by_player
            .iter()
            .enumerate()
            .filter(|(player_id, _)| *player_id != own_player)
            .filter_map(|(_, pos)| *pos)
        {
            for direction in Direction::all_directions() {
                let new_pos = self.offset_pos(pos, direction);
                mask[new_pos.1 * self.width + new_pos.0] = true;
            }
        }
        mask
    }
}
