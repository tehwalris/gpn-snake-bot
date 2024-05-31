use crate::Direction;

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
        let (x, y) = pos;
        let x = (x as isize
            + match direction {
                Direction::Left => -1,
                Direction::Right => 1,
                _ => 0,
            }
            + self.width as isize) as usize
            % self.width;
        let y = (y as isize
            + match direction {
                Direction::Up => -1,
                Direction::Down => 1,
                _ => 0,
            }
            + self.height as isize) as usize
            % self.height;
        (x, y)
    }
}
