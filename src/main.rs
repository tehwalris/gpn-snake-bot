#![allow(dead_code)]

use anyhow::anyhow;
use anyhow::Result;
use core::time;
use float_ord::FloatOrd;
use image::ImageBuffer;
use image::Luma;
use rand::distributions::WeightedIndex;
use rand::prelude::Distribution;
use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::fs::File;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    fn step(&mut self, pos: &PosWithWalls) -> Result<Direction>;
}

struct FirstPossibleStrategy {}

impl FirstPossibleStrategy {
    fn new(_goal: (i32, i32)) -> Self {
        Self {}
    }
}

impl Strategy for FirstPossibleStrategy {
    fn step(&mut self, pos: &PosWithWalls) -> Result<Direction> {
        pos.possible_dirs()
            .first()
            .cloned()
            .ok_or(anyhow!("no valid direction to move"))
    }
}

struct RandomPossibleStrategy {}

impl RandomPossibleStrategy {
    fn new(_goal: (i32, i32)) -> Self {
        Self {}
    }
}

impl Strategy for RandomPossibleStrategy {
    fn step(&mut self, pos: &PosWithWalls) -> Result<Direction> {
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
    fn new(goal: (i32, i32)) -> Self {
        Self {
            goal,
            visited_positions: HashSet::new(),
            entry_by_position: HashMap::new(),
        }
    }

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
    fn step(&mut self, old_pos: &PosWithWalls) -> Result<Direction> {
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
    distances: HashMap<(i32, i32), i32>,
}

impl DFSStrategy {
    fn new(goal: (i32, i32)) -> Self {
        Self {
            goal,
            added_positions: HashSet::new(),
            stack: Vec::new(),
            path_from_root: Vec::new(),
            distances: HashMap::new(),
        }
    }
}

impl Strategy for DFSStrategy {
    fn step(&mut self, old_pos: &PosWithWalls) -> Result<Direction> {
        let mut possible_dirs = old_pos.possible_dirs();
        let old_pos = (old_pos.x, old_pos.y);
        // reverse because this will be reversed again when pushing onto the stack
        possible_dirs.shuffle(&mut thread_rng());
        possible_dirs.sort_by_key(|d| -> std::cmp::Reverse<FloatOrd<f32>> {
            std::cmp::Reverse(FloatOrd(euclidean_distance(d.offset(old_pos), self.goal)))
        });

        self.distances
            .insert(old_pos, self.path_from_root.len() as i32);

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

struct ImprovedDFSStrategy {
    goal: (i32, i32),
    added_positions: HashSet<(i32, i32)>,
    stack: Vec<(i32, i32)>,
    path_from_root: Vec<Direction>,
    original_distances: Vec<f32>,
    original_distances_size: (i32, i32),
    scaled_distances: Vec<f32>,
    estimated_size: (i32, i32),
}

impl ImprovedDFSStrategy {
    fn new(goal: (i32, i32), distances: &Vec<f32>, distances_size: (i32, i32)) -> Self {
        Self {
            goal,
            added_positions: HashSet::new(),
            stack: Vec::new(),
            path_from_root: Vec::new(),
            original_distances: distances.clone(),
            original_distances_size: distances_size,
            scaled_distances: distances.clone(),
            estimated_size: distances_size,
        }
    }

    fn estimate_distance(&self, pos: (i32, i32)) -> f32 {
        assert!(self.goal == (0, 0));
        self.scaled_distances[(pos.1 * self.estimated_size.0 + pos.0) as usize]
    }
}

impl Strategy for ImprovedDFSStrategy {
    fn step(&mut self, old_pos: &PosWithWalls) -> Result<Direction> {
        if self.goal != (0, 0) {
            return Err(anyhow!("unsupported goal"));
        }

        let mut possible_dirs = old_pos.possible_dirs();
        let old_pos = (old_pos.x, old_pos.y);

        let estimated_size = (
            i32::max(old_pos.1 + 1, self.estimated_size.0),
            i32::max(old_pos.1 + 1, self.estimated_size.1),
        );
        if estimated_size != self.estimated_size {
            self.estimated_size = estimated_size;
            let original_distances = ImageBuffer::<Luma<f32>, Vec<f32>>::from_raw(
                self.original_distances_size.0 as u32,
                self.original_distances_size.1 as u32,
                self.original_distances.clone(),
            )
            .ok_or(anyhow!("failed to create image"))?;
            let scaled_distances = image::imageops::resize(
                &original_distances,
                estimated_size.0 as u32,
                estimated_size.1 as u32,
                image::imageops::FilterType::Lanczos3,
            );
            self.scaled_distances = scaled_distances.to_vec();
        }

        // reverse because this will be reversed again when pushing onto the stack
        possible_dirs.shuffle(&mut thread_rng());
        possible_dirs.sort_by_key(|d| -> std::cmp::Reverse<FloatOrd<f32>> {
            std::cmp::Reverse(FloatOrd(self.estimate_distance(d.offset(old_pos))))
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

fn run_round<S: Strategy, F: FnOnce((i32, i32)) -> S, R: Read, W: Write>(
    make_strategy: F,
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
    let mut strategy = make_strategy(goal.unwrap());

    loop {
        let old_pos = pos.unwrap();
        let direction = strategy.step(&old_pos)?;
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

fn try_play(
    username: String,
    password: String,
    distances: &Vec<f32>,
    distances_size: (i32, i32),
) -> Result<()> {
    let stream = TcpStream::connect("gpn-mazing.v6.rocks:4000")?;
    let mut reader = GameReader::new(&stream);
    let mut writer = GameWriter::new(&stream);

    writer.write(&ClientMessage::Join {
        username: username.clone(),
        password: password.clone(),
    })?;

    run_round(
        |goal| ImprovedDFSStrategy::new(goal, distances, distances_size),
        &mut reader,
        &mut writer,
    )?;

    Ok(())
}

fn run_online(distances: &Vec<f32>, distances_size: (i32, i32)) -> Result<()> {
    let username = std::env::var("GPN_MAZING_USERNAME").expect("GPN_MAZING_USERNAME is not set");
    let password = std::env::var("GPN_MAZING_PASSWORD").expect("GPN_MAZING_PASSWORD is not set");

    loop {
        println!(
            "restarting due to error: {:?}",
            try_play(
                username.clone(),
                password.clone(),
                distances,
                distances_size,
            )
        );
        std::thread::sleep(time::Duration::from_millis(200));
    }
}

type OfflineMaze = Vec<PosWithWalls>;

fn calculate_maze_size(maze: &OfflineMaze) -> (i32, i32) {
    let max = maze.iter().map(|p| (p.x, p.y)).max().unwrap();
    let size = (max.0 + 1, max.1 + 1);
    assert!(maze.len() == (size.0 * size.1) as usize);
    size
}

fn run_offline<
    S: Strategy,
    FMakeStrategy: Fn((i32, i32)) -> S,
    FGetDistances: Fn(S) -> Option<HashMap<(i32, i32), i32>>,
    FGetStartPos: Fn((i32, i32)) -> (i32, i32),
    FGetGoal: Fn((i32, i32)) -> (i32, i32),
>(
    make_strategy: FMakeStrategy,
    get_distances: FGetDistances,
    get_start_pos: FGetStartPos,
    get_goal: FGetGoal,
) -> Result<Option<(Vec<f32>, (i32, i32))>> {
    let mazes: Vec<OfflineMaze> =
        serde_json::from_reader(BufReader::new(File::open("mazes/mazes.json")?))?;
    println!("loaded {} mazes", mazes.len());

    let mut size = None;
    for maze in &mazes {
        let this_size = calculate_maze_size(maze);
        assert!(size.is_none() || size == Some(this_size));
        size = Some(this_size);
    }
    let size = size.unwrap();

    let mut distances_sum = None;
    let mut steps_until_goal_sum = 0;

    for maze in &mazes {
        let inputs_by_position: HashMap<_, _> = maze
            .iter()
            .map(|input| ((input.x, input.y), input.clone()))
            .collect();

        let max_pos = maze.iter().map(|p| (p.x, p.y)).max().unwrap();

        let goal = get_goal(max_pos);
        let mut goal_reached = false;
        let mut strategy = make_strategy(goal);
        let mut pos = get_start_pos(max_pos);
        let mut steps_until_goal = 0;
        loop {
            if !goal_reached {
                steps_until_goal += 1;
            }
            match strategy.step(&inputs_by_position[&pos]) {
                Ok(dir) => pos = dir.offset(pos),
                Err(_) => break,
            }
            if pos == goal {
                goal_reached = true;
            }
        }

        steps_until_goal_sum += steps_until_goal;

        let distances = get_distances(strategy);
        assert!(distances_sum.is_none() || distances.is_some());
        if distances_sum.is_none() && distances.is_some() {
            distances_sum = Some(vec![0; (size.0 * size.1) as usize]);
        }
        if let Some(distances) = distances {
            for y in 0..size.1 {
                for x in 0..size.0 {
                    distances_sum.as_deref_mut().unwrap()[(size.0 * y + x) as usize] +=
                        distances.get(&(x, y)).unwrap();
                }
            }
        }
    }

    let steps_until_goal_mean = (steps_until_goal_sum as f32) / (mazes.len() as f32);
    println!("steps_until_goal_mean: {} ", steps_until_goal_mean);

    let distances_mean: Option<Vec<_>> = distances_sum.map(|distances_sum| {
        distances_sum
            .into_iter()
            .map(|v| (v as f32) / (mazes.len() as f32))
            .collect()
    });

    if let Some(distances_mean) = distances_mean {
        let max_distances_mean = *distances_mean.iter().max_by_key(|v| FloatOrd(**v)).unwrap();
        let debug_img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(
            size.0 as u32,
            size.1 as u32,
            distances_mean
                .iter()
                .map(|v| (v / max_distances_mean * 255.0) as u8)
                .collect(),
        )
        .ok_or(anyhow!("failed to create image"))?;
        debug_img.save("output/distances_mean.png")?;

        Ok(Some((distances_mean, size)))
    } else {
        Ok(None)
    }
}

fn main() -> Result<()> {
    let (distances, distances_size) = run_offline(
        |goal| DFSStrategy::new(goal),
        |s| Some(s.distances),
        |_| (0, 0),
        |p| p,
    )?
    .unwrap();
    let _ = run_offline(|goal| DFSStrategy::new(goal), |_| None, |p| p, |_| (0, 0))?;
    let _ = run_offline(
        |goal| ImprovedDFSStrategy::new(goal, &distances, distances_size),
        |_| None,
        |p| p,
        |_| (0, 0),
    )?;
    run_online(&distances, distances_size)?;

    Ok(())
}
