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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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

    fn get_wall_in_dir(&self, dir: Direction) -> bool {
        match dir {
            Direction::Up => self.top,
            Direction::Right => self.right,
            Direction::Down => self.bottom,
            Direction::Left => self.left,
        }
    }

    fn set_wall_in_dir(&mut self, dir: Direction, value: bool) {
        let variable = match dir {
            Direction::Up => &mut self.top,
            Direction::Right => &mut self.right,
            Direction::Down => &mut self.bottom,
            Direction::Left => &mut self.left,
        };
        *variable = value;
    }
}

#[derive(Debug)]
enum ServerMessage {
    Game {
        width: i32,
        height: i32,
        goal_x: i32,
        goal_y: i32,
    },
    Motd {
        message: String,
    },
    Error {
        message: String,
    },
    Goal {
        x: i32,
        y: i32,
    },
    Pos(PosWithWalls),
    Win {
        wins: i32,
        losses: i32,
    },
    Lose {
        wins: i32,
        losses: i32,
    },
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
            ["game", width, height, goal_x, goal_y] => Ok(ServerMessage::Game {
                width: width.parse()?,
                height: height.parse()?,
                goal_x: goal_x.parse()?,
                goal_y: goal_y.parse()?,
            }),
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

struct DistanceMapCache<'a> {
    mazes: &'a Vec<OfflineMaze>,
    distances_by_goal: HashMap<(i32, i32), (Vec<f32>, (i32, i32))>,
}

impl<'a> DistanceMapCache<'a> {
    fn new(mazes: &'a Vec<OfflineMaze>) -> DistanceMapCache<'a> {
        DistanceMapCache {
            mazes,
            distances_by_goal: HashMap::new(),
        }
    }

    fn get_or_calculate<F: Fn((i32, i32)) -> (i32, i32)>(
        &mut self,
        get_goal: F,
    ) -> &(Vec<f32>, (i32, i32)) {
        let size = calculate_maze_size(&self.mazes[0]);
        let goal = get_goal((size.0 - 1, size.1 - 1));

        if !self.distances_by_goal.contains_key(&goal) {
            let (distances, actual_size) = run_offline(
                self.mazes,
                |goal| DFSStrategy::new(goal),
                |s| Some(s.distances),
                |_| goal,
                |_| (0, 0),
            )
            .unwrap()
            .unwrap();
            assert!(actual_size == size);
            let distances_max = *distances.iter().max_by_key(|v| FloatOrd(**v)).unwrap();
            let distances = distances.into_iter().map(|v| v / distances_max).collect();
            self.distances_by_goal.insert(goal, (distances, size));
        }

        self.distances_by_goal.get(&goal).unwrap()
    }
}

fn extend_assuming_no_more_walls(
    known_inputs: &HashMap<(i32, i32), PosWithWalls>,
    size: (i32, i32),
) -> HashMap<(i32, i32), PosWithWalls> {
    let mut inputs = known_inputs.clone();

    let mut add_fake_boundary_input = |x, y| {
        inputs.insert(
            (x, y),
            PosWithWalls {
                x,
                y,
                top: true,
                right: true,
                bottom: true,
                left: true,
            },
        )
    };
    for x in -1..=size.0 {
        add_fake_boundary_input(x, -1);
        add_fake_boundary_input(x, size.1);
    }
    for y in -1..=size.0 {
        add_fake_boundary_input(-1, y);
        add_fake_boundary_input(size.0, y);
    }

    for x in 0..size.0 {
        for y in 0..size.1 {
            if inputs.contains_key(&(x, y)) {
                continue;
            }

            let wall_from_neighbor = |d: Direction| match inputs.get(&d.offset((x, y))) {
                Some(neighbor_input) => neighbor_input.get_wall_in_dir(d.reverse()),
                None => false,
            };

            let mut fake_input = PosWithWalls {
                x,
                y,
                top: false,
                right: false,
                bottom: false,
                left: false,
            };
            for d in [
                Direction::Up,
                Direction::Right,
                Direction::Down,
                Direction::Left,
            ] {
                fake_input.set_wall_in_dir(d, wall_from_neighbor(d));
            }

            inputs.insert((x, y), fake_input);
        }
    }

    for input in known_inputs.clone().into_iter() {
        assert!(known_inputs.get(&input.0).cloned() == Some(input.1));
    }

    inputs
}

fn determine_reachable(
    inputs: HashMap<(i32, i32), PosWithWalls>,
    start_pos: (i32, i32),
) -> HashSet<(i32, i32)> {
    let mut stack = Vec::new();
    stack.push(start_pos);
    let mut added = HashSet::new();
    while let Some(pos) = stack.pop() {
        for d in inputs.get(&pos).unwrap().possible_dirs() {
            let new_pos = d.offset(pos);
            if !added.contains(&new_pos) {
                stack.push(new_pos);
            }
        }
        added.insert(pos);
    }

    added
}

struct ImprovedDFSStrategy<'a> {
    goal: (i32, i32),
    inputs: HashMap<(i32, i32), PosWithWalls>,
    added_positions: HashSet<(i32, i32)>,
    backtracked_positions: HashSet<(i32, i32)>,
    stack: Vec<(i32, i32)>,
    path_from_root: Vec<Direction>,
    estimated_size: (i32, i32),
    mazes_by_size: &'a HashMap<usize, Vec<OfflineMaze>>,
}

impl<'a> ImprovedDFSStrategy<'a> {
    fn new(goal: (i32, i32), mazes_by_size: &'a HashMap<usize, Vec<OfflineMaze>>) -> Self {
        Self {
            goal,
            inputs: HashMap::new(),
            added_positions: HashSet::new(),
            backtracked_positions: HashSet::new(),
            stack: Vec::new(),
            path_from_root: Vec::new(),
            estimated_size: (0, 0),
            mazes_by_size,
        }
    }
}

impl<'a> Strategy for ImprovedDFSStrategy<'a> {
    fn step(&mut self, old_pos: &PosWithWalls) -> Result<Direction> {
        let mut possible_dirs = old_pos.possible_dirs();
        self.inputs.insert((old_pos.x, old_pos.y), old_pos.clone());
        let old_pos = (old_pos.x, old_pos.y);

        self.estimated_size = (
            i32::max(old_pos.1 + 1, self.estimated_size.0),
            i32::max(old_pos.1 + 1, self.estimated_size.1),
        );
        assert!(self.estimated_size.0 == self.estimated_size.1);
        let mazes: Vec<OfflineMaze> =
            if let Some(mazes) = self.mazes_by_size.get(&(self.estimated_size.0 as usize)) {
                mazes
                    .iter()
                    .map(|mut maze| -> OfflineMaze {
                        maze.iter()
                            .map(|p| self.inputs.get(&(p.x, p.y)).cloned().unwrap_or(p.clone()))
                            .collect()
                    })
                    .collect()
            } else {
                return Err(anyhow!("no mazes of size {}", self.estimated_size.0));
            };
        let (distances, actual_size) = run_offline(
            &mazes,
            |goal| DFSStrategy::new(goal),
            |s| Some(s.distances),
            |_| self.goal,
            |_| (0, 0),
        )
        .unwrap()
        .unwrap();
        assert!(actual_size == self.estimated_size);
        save_debug_image("distances_mean", &distances, self.estimated_size)?;

        let mut extended_inputs = extend_assuming_no_more_walls(&self.inputs, self.estimated_size);
        let mut block_cell = |p: (i32, i32)| {
            extended_inputs.insert(
                old_pos,
                PosWithWalls {
                    x: p.0,
                    y: p.1,
                    top: true,
                    right: true,
                    bottom: true,
                    left: true,
                },
            );
            for d in [
                Direction::Up,
                Direction::Right,
                Direction::Down,
                Direction::Left,
            ] {
                extended_inputs
                    .get_mut(&d.offset(p))
                    .unwrap()
                    .set_wall_in_dir(d.reverse(), true);
            }
        };
        block_cell(old_pos);
        {
            let mut p = old_pos;
            for d in self.path_from_root.iter().rev() {
                p = d.reverse().offset(p);
                block_cell(p);
            }
        }
        for p in &self.backtracked_positions {
            block_cell(*p);
        }

        let positions_that_can_reach_goal = determine_reachable(extended_inputs, self.goal);

        let mut fake_image = vec![0.0; (self.estimated_size.0 * self.estimated_size.1) as usize];
        for (x, y) in &positions_that_can_reach_goal {
            fake_image[(y * self.estimated_size.0 + x) as usize] = 1.0;
        }
        save_debug_image("left_to_explore", &fake_image, self.estimated_size)?;

        possible_dirs = possible_dirs
            .into_iter()
            .filter(|d| {
                let can_reach = positions_that_can_reach_goal.contains(&d.offset(old_pos));
                if !can_reach {
                    println!("ignoring unreachable direction: {:?}", d);
                }
                can_reach
            })
            .collect();

        // reverse because this will be reversed again when pushing onto the stack
        possible_dirs.shuffle(&mut thread_rng());
        possible_dirs.sort_by_key(|d| -> std::cmp::Reverse<FloatOrd<f32>> {
            let p = d.offset(old_pos);
            std::cmp::Reverse(FloatOrd(
                distances
                    .get((p.1 * self.estimated_size.0 + p.0) as usize)
                    .cloned()
                    .unwrap_or(f32::INFINITY),
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

        while let Some(p) = self.stack.last() {
            if positions_that_can_reach_goal.contains(p) {
                break;
            }
            self.stack.pop();
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
            self.backtracked_positions.insert(old_pos);
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
            ServerMessage::Game { .. } => (),
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
                ServerMessage::Game { .. } => (),
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
    host_port: String,
    username: String,
    password: String,
    mazes_by_size: &HashMap<usize, Vec<OfflineMaze>>,
) -> Result<()> {
    println!("connecting");

    let stream = TcpStream::connect(host_port)?;
    let mut reader = GameReader::new(&stream);
    let mut writer = GameWriter::new(&stream);

    writer.write(&ClientMessage::Join {
        username: username.clone(),
        password: password.clone(),
    })?;

    run_round(
        |goal| ImprovedDFSStrategy::new(goal, &mazes_by_size),
        &mut reader,
        &mut writer,
    )?;

    Ok(())
}

fn run_online(mazes_by_size: &HashMap<usize, Vec<OfflineMaze>>) -> Result<()> {
    let host_port =
        std::env::var("GPN_MAZING_HOST_PORT").unwrap_or("gpn-mazing.v6.rocks:4000".to_string());
    let username = std::env::var("GPN_MAZING_USERNAME").expect("GPN_MAZING_USERNAME is not set");
    let password = std::env::var("GPN_MAZING_PASSWORD").expect("GPN_MAZING_PASSWORD is not set");

    loop {
        println!(
            "restarting due to error: {:?}",
            try_play(
                host_port.clone(),
                username.clone(),
                password.clone(),
                mazes_by_size
            ),
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
    mazes: &Vec<OfflineMaze>,
    make_strategy: FMakeStrategy,
    get_distances: FGetDistances,
    get_start_pos: FGetStartPos,
    get_goal: FGetGoal,
) -> Result<Option<(Vec<f32>, (i32, i32))>> {
    let mut size = None;
    for maze in mazes {
        let this_size = calculate_maze_size(maze);
        assert!(size.is_none() || size == Some(this_size));
        size = Some(this_size);
    }
    let size = size.unwrap();

    let mut distances_sum = None;
    let mut steps_until_goal_sum = 0;

    for maze in mazes {
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
                    distances_sum.as_deref_mut().unwrap()[(size.0 * y + x) as usize] += distances
                        .get(&(x, y))
                        .cloned()
                        .unwrap_or(size.0 * size.1 + 1);
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
        save_debug_image("distances_mean", &distances_mean, size)?;
        Ok(Some((distances_mean, size)))
    } else {
        Ok(None)
    }
}

fn save_debug_image(name: &str, values: &Vec<f32>, size: (i32, i32)) -> Result<()> {
    let prefix = std::env::var("GPN_MAZING_DEBUG_PREFIX").unwrap_or("default".to_string());

    let max = *values.iter().max_by_key(|v| FloatOrd(**v)).unwrap();
    let debug_img = ImageBuffer::<Luma<u8>, Vec<u8>>::from_raw(
        size.0 as u32,
        size.1 as u32,
        values.iter().map(|v| (v / max * 255.0) as u8).collect(),
    )
    .ok_or(anyhow!("failed to create image"))?;
    debug_img.save(format!("output/{}_{}.png", prefix, name))?;
    Ok(())
}

fn main() -> Result<()> {
    let mut mazes_by_size = HashMap::new();
    for size in 10..40 {
        let mazes: Vec<OfflineMaze> = serde_json::from_reader(BufReader::new(File::open(
            format!("mazes/mazes_{}.json", size),
        )?))?;
        println!("loaded {} mazes for size {}", mazes.len(), size);
        mazes_by_size.insert(size, mazes);
    }

    // let distance_map_cache = Mutex::new(DistanceMapCache::new(&mazes));
    //
    // println!("DFSStrategy down");
    // let _ = run_offline(
    //     &mazes,
    //     |goal| DFSStrategy::new(goal),
    //     |s| Some(s.distances),
    //     |_| (0, 0),
    //     |p| p,
    // )?
    // .unwrap();
    // println!("DFSStrategy up");
    // let _ = run_offline(
    //     &mazes,
    //     |goal| DFSStrategy::new(goal),
    //     |_| None,
    //     |p| p,
    //     |_| (0, 0),
    // )?;
    // println!("ImprovedDFSStrategy up");
    // let _ = run_offline(
    //     &mazes,
    //     |goal| ImprovedDFSStrategy::new(goal, &mazes, &distance_map_cache),
    //     |_| None,
    //     |p| p,
    //     |_| (0, 0),
    // )?;
    run_online(&mazes_by_size)?;

    Ok(())
}
