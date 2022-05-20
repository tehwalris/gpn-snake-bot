use anyhow::anyhow;
use anyhow::Result;
use core::time;
use float_ord::FloatOrd;
use rand::distributions::Uniform;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fmt;
use std::ops::Add;
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    net::TcpStream,
};

#[derive(Debug, Clone, Copy)]
enum Direction {
    Up,
    Right,
    Down,
    Left,
}

impl Direction {
    fn offset(&self, pos: (i32, i32)) -> (i32, i32) {
        match self {
            Direction::Up => (pos.0, pos.1 - 1),
            Direction::Right => (pos.0 + 1, pos.1),
            Direction::Down => (pos.0, pos.1 + 1),
            Direction::Left => (pos.0 - 1, pos.1),
        }
    }

    fn reverse(&self) -> Direction {
        match self {
            Direction::Up => Direction::Down,
            Direction::Right => Direction::Left,
            Direction::Down => Direction::Up,
            Direction::Left => Direction::Right,
        }
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

#[derive(Debug, Clone)]
struct PosWithWalls {
    x: i32,
    y: i32,
    top: bool,
    right: bool,
    bottom: bool,
    left: bool,
}

impl PosWithWalls {
    fn possible_dirs(&self) -> Vec<Direction> {
        let mut possible_dirs = Vec::new();
        if !self.top {
            possible_dirs.push(Direction::Up);
        }
        if !self.right {
            possible_dirs.push(Direction::Right);
        }
        if !self.bottom {
            possible_dirs.push(Direction::Down);
        }
        if !self.left {
            possible_dirs.push(Direction::Left);
        }
        possible_dirs
    }
}

#[derive(Debug)]
enum ServerMessage {
    Motd { message: String },
    Error { message: String },
    Goal { x: i32, y: i32 },
    Pos(PosWithWalls),
    Win { wins: i32, losses: i32 },
    Lose { wins: i32, losses: i32 },
}

#[derive(Debug)]
enum ClientMessage {
    Join { username: String, password: String },
    Move { direction: Direction },
    Chat { message: String },
}

struct GameReader<R: Read> {
    inner: BufReader<R>,
    line: String,
}

fn parse_01_string(s: &str) -> Result<bool> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(anyhow!("expected 0 or 1, but got: {}", s)),
    }
}

impl<R: Read> GameReader<R> {
    fn new(inner: R) -> GameReader<R> {
        GameReader {
            inner: BufReader::new(inner),
            line: String::new(),
        }
    }

    fn read(&mut self) -> Result<ServerMessage> {
        self.line.clear();
        let bytes_read = self.inner.read_line(&mut self.line)?;
        if bytes_read == 0 || self.line.chars().last().unwrap() != '\n' {
            return Err(anyhow::Error::new(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "unexpected EOF",
            )));
        }
        self.line.pop();

        let parts = self.line.split('|').collect::<Vec<_>>();
        match parts[..] {
            ["motd", message] => Ok(ServerMessage::Motd {
                message: message.into(),
            }),
            ["error", message] => Ok(ServerMessage::Error {
                message: message.into(),
            }),
            ["goal", x, y] => Ok(ServerMessage::Goal {
                x: x.parse()?,
                y: y.parse()?,
            }),
            ["pos", x, y, top, right, bottom, left] => Ok(ServerMessage::Pos(PosWithWalls {
                x: x.parse()?,
                y: y.parse()?,
                top: parse_01_string(top)?,
                right: parse_01_string(right)?,
                bottom: parse_01_string(bottom)?,
                left: parse_01_string(left)?,
            })),
            ["win", wins, losses] => Ok(ServerMessage::Win {
                wins: wins.parse()?,
                losses: losses.parse()?,
            }),
            ["lose", wins, losses] => Ok(ServerMessage::Lose {
                wins: wins.parse()?,
                losses: losses.parse()?,
            }),
            _ => Err(anyhow!("invalid message: {}", self.line)),
        }
    }
}

struct GameWriter<W: Write> {
    inner: BufWriter<W>,
}

impl<W: Write> GameWriter<W> {
    fn new(inner: W) -> GameWriter<W> {
        GameWriter {
            inner: BufWriter::new(inner),
        }
    }

    fn write(&mut self, message: &ClientMessage) -> Result<()> {
        self.inner.write(Self::encode_message(message).as_bytes())?;
        self.inner.flush()?;
        Ok(())
    }

    fn encode_message(message: &ClientMessage) -> String {
        match message {
            ClientMessage::Join { username, password } => {
                format!("join|{}|{}\n", username, password)
            }
            ClientMessage::Move { direction } => format!("move|{}\n", direction),
            ClientMessage::Chat { message } => format!("chat|{}\n", message),
        }
    }
}

trait Strategy: Sized {
    fn start(goal: (i32, i32)) -> Self;
    fn step(&mut self, pos: PosWithWalls) -> Result<Direction>;
}

struct FirstPossibleStrategy {}

impl Strategy for FirstPossibleStrategy {
    fn start(_goal: (i32, i32)) -> Self {
        Self {}
    }

    fn step(&mut self, pos: PosWithWalls) -> Result<Direction> {
        pos.possible_dirs()
            .first()
            .cloned()
            .ok_or(anyhow!("no valid direction to move"))
    }
}

struct RandomPossibleStrategy {}

impl Strategy for RandomPossibleStrategy {
    fn start(_goal: (i32, i32)) -> Self {
        Self {}
    }

    fn step(&mut self, pos: PosWithWalls) -> Result<Direction> {
        if let Some(&dir) = pos.possible_dirs().choose(&mut thread_rng()) {
            Ok(dir)
        } else {
            Err(anyhow!("no valid direction to move"))
        }
    }
}

enum ProgressTowardsGoal {
    Closer,
    Same,
    Further,
}

fn manhattan_distance(a: (i32, i32), b: (i32, i32)) -> i32 {
    (a.0 - b.0).abs() + (a.1 - b.1).abs()
}

fn euclidean_distance(a: (i32, i32), b: (i32, i32)) -> f32 {
    f32::sqrt(f32::powi(a.0 as f32 - b.0 as f32, 2) + f32::powi(a.1 as f32 - b.1 as f32, 2))
}

struct WeightedRandomStrategy {
    goal: (i32, i32),
    visited_positions: HashSet<(i32, i32)>,
    entry_by_position: HashMap<(i32, i32), (i32, i32)>,
}

impl WeightedRandomStrategy {
    fn estimate_progress(&self, old_pos: (i32, i32), new_pos: (i32, i32)) -> ProgressTowardsGoal {
        let old_dist = manhattan_distance(old_pos, self.goal);
        let new_dist = manhattan_distance(new_pos, self.goal);
        if new_dist < old_dist {
            return ProgressTowardsGoal::Closer;
        } else if new_dist > old_dist {
            return ProgressTowardsGoal::Further;
        } else {
            return ProgressTowardsGoal::Same;
        }
    }
}

impl Strategy for WeightedRandomStrategy {
    fn start(goal: (i32, i32)) -> Self {
        Self {
            goal,
            visited_positions: HashSet::new(),
            entry_by_position: HashMap::new(),
        }
    }

    fn step(&mut self, old_pos: PosWithWalls) -> Result<Direction> {
        let possible_dirs = old_pos.possible_dirs();
        let old_pos = (old_pos.x, old_pos.y);

        let weights: Vec<f32> = possible_dirs
            .iter()
            .map(|d| {
                let new_pos = d.offset(old_pos);
                let mut w = 1.0;

                match self.estimate_progress(old_pos, new_pos) {
                    ProgressTowardsGoal::Closer => w *= 1.2,
                    ProgressTowardsGoal::Same => (),
                    ProgressTowardsGoal::Further => w *= 0.9,
                }

                if !self.visited_positions.contains(&new_pos) {
                    w *= 4.0;
                }

                if self.entry_by_position.get(&old_pos).cloned() == Some(new_pos) {
                    w *= 1.7;
                }

                w
            })
            .collect::<Vec<_>>();

        let chosen_dir = possible_dirs[WeightedIndex::new(weights)?.sample(&mut thread_rng())];
        let new_pos = chosen_dir.offset(old_pos);

        self.visited_positions.insert(new_pos);
        if !self.entry_by_position.contains_key(&new_pos) {
            self.entry_by_position.insert(new_pos, old_pos);
        }

        Ok(chosen_dir)
    }
}

struct DFSStrategy {
    goal: (i32, i32),
    added_positions: HashSet<(i32, i32)>,
    stack: Vec<(i32, i32)>,
    path_from_root: Vec<Direction>,
}

impl Strategy for DFSStrategy {
    fn start(goal: (i32, i32)) -> Self {
        Self {
            goal,
            added_positions: HashSet::new(),
            stack: Vec::new(),
            path_from_root: Vec::new(),
        }
    }

    fn step(&mut self, old_pos: PosWithWalls) -> Result<Direction> {
        let mut possible_dirs = old_pos.possible_dirs();
        let old_pos = (old_pos.x, old_pos.y);
        // reverse because this will be reversed again when pushing onto the stack
        possible_dirs.sort_by_key(|d| -> std::cmp::Reverse<FloatOrd<f32>> {
            std::cmp::Reverse(FloatOrd(
                euclidean_distance(d.offset(old_pos), self.goal)
                    + Uniform::new(0.0, 0.85).sample(&mut thread_rng()),
            ))
        });

        self.added_positions.insert(old_pos); // for the initial cell
        for d in &possible_dirs {
            let new_pos = d.offset(old_pos);
            if !self.added_positions.contains(&new_pos) {
                self.added_positions.insert(new_pos);
                self.stack.push(new_pos);
            }
        }

        let target_new_pos = self
            .stack
            .last()
            .ok_or(anyhow!("nothing left to explore"))?;
        let target_direction = possible_dirs
            .iter()
            .find(|d| d.offset(old_pos) == *target_new_pos)
            .cloned();

        if let Some(target_direction) = target_direction {
            self.stack.pop();
            self.path_from_root.push(target_direction);
            Ok(target_direction)
        } else {
            let back_direction = self
                .path_from_root
                .pop()
                .ok_or(anyhow!("nothing left to backtrack"))?
                .reverse();
            Ok(back_direction)
        }
    }
}

fn run_round<R: Read, W: Write, S: Strategy>(
    reader: &mut GameReader<R>,
    writer: &mut GameWriter<W>,
) -> Result<()> {
    println!("waiting for next round");

    let mut goal: Option<(i32, i32)> = None;
    let mut pos: Option<PosWithWalls> = None;
    while goal.is_none() || pos.is_none() {
        let msg = reader.read()?;
        println!("{:?}", msg);

        match msg {
            ServerMessage::Motd { .. } => (),
            ServerMessage::Error { .. } => return Ok(()),
            ServerMessage::Goal { x, y } => goal = Some((x, y)),
            ServerMessage::Pos(new_pos) => pos = Some(new_pos),
            ServerMessage::Win { .. } => return Ok(()),
            ServerMessage::Lose { .. } => return Ok(()),
        };
    }

    println!("starting round");
    let mut strategy = S::start(goal.unwrap());

    loop {
        let old_pos = pos.unwrap();
        let direction = strategy.step(old_pos.clone())?;
        println!("moving {}", direction);
        writer.write(&ClientMessage::Move { direction })?;

        loop {
            let msg = reader.read()?;
            println!("{:?}", msg);

            match msg {
                ServerMessage::Motd { .. } => (),
                ServerMessage::Error { .. } => return Ok(()),
                ServerMessage::Goal { .. } => return Ok(()),
                ServerMessage::Pos(new_pos) => {
                    let expected_pos = direction.offset((old_pos.x, old_pos.y));
                    if (new_pos.x, new_pos.y) != expected_pos {
                        return Err(anyhow!("unexpected position after move"));
                    }
                    pos = Some(new_pos);
                    break;
                }
                ServerMessage::Win { .. } => return Ok(()),
                ServerMessage::Lose { .. } => return Ok(()),
            };
        }
    }
}

fn try_play(username: String, password: String) -> Result<()> {
    let stream = TcpStream::connect("gpn-mazing.v6.rocks:4000")?;
    let mut reader = GameReader::new(&stream);
    let mut writer = GameWriter::new(&stream);

    writer.write(&ClientMessage::Join {
        username: username.clone(),
        password: password.clone(),
    })?;

    run_round::<_, _, DFSStrategy>(&mut reader, &mut writer)?;

    Ok(())
}

fn main() -> Result<()> {
    let username = std::env::var("GPN_MAZING_USERNAME").expect("GPN_MAZING_USERNAME is not set");
    let password = std::env::var("GPN_MAZING_PASSWORD").expect("GPN_MAZING_PASSWORD is not set");

    loop {
        println!(
            "restarting due to error: {:?}",
            try_play(username.clone(), password.clone())
        );
        std::thread::sleep(time::Duration::from_millis(200));
    }
}
