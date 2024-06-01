use crate::direction::Direction;

#[derive(Clone)]
struct BoardTrackerPlayer {
    latest_pos: Option<(usize, usize)>,
    dead: bool,
}

#[derive(Clone)]
pub struct BoardTracker {
    width: usize,
    height: usize,
    board: Vec<usize>,
    players: Vec<BoardTrackerPlayer>,
}

impl BoardTracker {
    const NO_PLAYER: usize = usize::MAX;

    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            board: vec![Self::NO_PLAYER; width * height],
            players: Vec::new(),
        }
    }

    pub fn board_size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    fn get_or_create_internal_player_mut(&mut self, player_id: usize) -> &mut BoardTrackerPlayer {
        if player_id >= self.players.len() {
            self.players.resize(
                player_id + 1,
                BoardTrackerPlayer {
                    latest_pos: None,
                    dead: false,
                },
            );
        }
        &mut self.players[player_id]
    }

    fn get_internal_player(&self, player_id: usize) -> &BoardTrackerPlayer {
        &self.players[player_id]
    }

    pub fn is_dead(&self, player_id: usize) -> bool {
        self.get_internal_player(player_id).dead
    }

    pub fn count_dead(&self) -> usize {
        self.players.iter().filter(|p| p.dead).count()
    }

    pub fn count_alive(&self) -> usize {
        self.players.iter().filter(|p| !p.dead).count()
    }

    pub fn count_seen(&self) -> usize {
        self.players.len()
    }

    pub fn get_cell_player(&self, (x, y): (usize, usize)) -> Option<usize> {
        match self.board[y * self.width + x] {
            Self::NO_PLAYER => None,
            player_id => Some(player_id),
        }
    }

    pub fn get_player_latest_pos(&self, player_id: usize) -> Option<(usize, usize)> {
        self.players.get(player_id).and_then(|p| p.latest_pos)
    }

    pub fn record_pos(&mut self, player_id: usize, (x, y): (usize, usize)) -> bool {
        let v = &mut self.board[y * self.width + x];
        let duplicate = *v != Self::NO_PLAYER;
        *v = player_id;

        let player = self.get_or_create_internal_player_mut(player_id);
        player.latest_pos = Some((x, y));
        player.dead = false;

        duplicate
    }

    pub fn record_death(&mut self, player_id: usize, clear: bool) {
        let player = self.get_or_create_internal_player_mut(player_id);
        player.dead = true;

        if clear {
            for i in 0..self.board.len() {
                if self.board[i] == player_id {
                    self.board[i] = Self::NO_PLAYER;
                }
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
            .players
            .iter()
            .enumerate()
            .filter(|(player_id, _)| *player_id != own_player)
            .filter_map(|(_, player)| player.latest_pos)
        {
            for direction in Direction::all_directions() {
                let new_pos = self.offset_pos(pos, direction);
                mask[new_pos.1 * self.width + new_pos.0] = true;
            }
        }
        mask
    }
}
