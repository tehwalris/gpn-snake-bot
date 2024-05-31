use core::fmt;

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    pub fn reverse(&self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Right => Direction::Left,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
        }
    }

    pub fn offset_pos(&self, pos: (usize, usize), size: (usize, usize)) -> (usize, usize) {
        let (x, y) = pos;
        let (width, height) = size;
        assert!(x < width && y < height, "Invalid position: ({}, {})", x, y);

        let x = (x as isize
            + match self {
                Direction::Left => -1,
                Direction::Right => 1,
                _ => 0,
            }
            + width as isize) as usize
            % width;
        let y = (y as isize
            + match self {
                Direction::Up => -1,
                Direction::Down => 1,
                _ => 0,
            }
            + height as isize) as usize
            % height;
        (x, y)
    }

    pub fn all_directions() -> [Direction; 4] {
        [
            Direction::Up,
            Direction::Right,
            Direction::Down,
            Direction::Left,
        ]
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Direction::Up => "up",
            Direction::Right => "right",
            Direction::Down => "down",
            Direction::Left => "left",
        })
    }
}
