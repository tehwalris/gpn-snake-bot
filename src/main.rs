#![allow(dead_code)]

use anyhow::anyhow;
use anyhow::Result;
use board_tracker::BoardTracker;
use core::time;
use direction::Direction;
use distance::calculate_distances;
use rand::prelude::SliceRandom;
use std::time::Duration;
use std::time::Instant;
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    net::TcpStream,
};

mod board_tracker;
mod direction;
mod distance;
mod playout;
mod reachability;
mod shortest_path;

#[derive(Debug)]
struct GameInfo {
    width: i32,
    height: i32,
    player_id: i32,
}

#[derive(Debug)]
enum ServerMessage {
    Motd { message: String },
    Error { message: String },
    Game { message: GameInfo },
    Pos { player_id: i32, x: i32, y: i32 },
    Player { player_id: i32, name: String },
    Tick,
    Die { player_ids: Vec<i32> },
    Message { player_id: i32, message: String },
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

        let parts: Vec<_> = self.line.split('|').collect();
        match parts[..] {
            ["motd", message] => Ok(ServerMessage::Motd {
                message: message.into(),
            }),
            ["error", message] => Ok(ServerMessage::Error {
                message: message.into(),
            }),
            ["game", width, height, player_id] => Ok(ServerMessage::Game {
                message: GameInfo {
                    width: width.parse()?,
                    height: height.parse()?,
                    player_id: player_id.parse()?,
                },
            }),
            ["pos", player_id, x, y] => Ok(ServerMessage::Pos {
                player_id: player_id.parse()?,
                x: x.parse()?,
                y: y.parse()?,
            }),
            ["player", player_id, name] => Ok(ServerMessage::Player {
                player_id: player_id.parse()?,
                name: name.into(),
            }),
            ["tick"] => Ok(ServerMessage::Tick),
            ["die", ..] => Ok(ServerMessage::Die {
                player_ids: parts[1..].iter().map(|id| id.parse().unwrap()).collect(),
            }),
            ["message", player_id, message] => Ok(ServerMessage::Message {
                player_id: player_id.parse()?,
                message: message.into(),
            }),
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

trait Strategy {
    fn start(&mut self, game_info: &GameInfo) -> ();
    fn step(&mut self, board: &BoardTracker, time_budget: Duration) -> Direction;
}

struct AlwaysDownStrategy {}

impl AlwaysDownStrategy {
    fn new() -> Self {
        Self {}
    }
}

impl Strategy for AlwaysDownStrategy {
    fn start(&mut self, _game_info: &GameInfo) -> () {}

    fn step(&mut self, _board: &BoardTracker, _time_budget: Duration) -> Direction {
        Direction::Down
    }
}

struct NoCrashRandomStrategy {
    player_id: usize,
}

impl NoCrashRandomStrategy {
    fn new() -> Self {
        Self { player_id: 0 }
    }
}

impl Strategy for NoCrashRandomStrategy {
    fn start(&mut self, game_info: &GameInfo) -> () {
        self.player_id = game_info.player_id as usize;
    }

    fn step(&mut self, board: &BoardTracker, _time_budget: Duration) -> Direction {
        let player_pos = board.get_player_latest_pos(self.player_id).unwrap();

        let mut directions = Direction::all_directions().to_vec();
        directions.shuffle(&mut rand::thread_rng());

        for direction in directions {
            let new_player_pos = board.offset_pos(player_pos, direction);

            if board.get_cell_player(new_player_pos).is_none() {
                return direction;
            }
        }

        Direction::Down
    }
}

struct ConstantThenOtherStrategy<T: Strategy> {
    did_first_step: bool,
    first_direction: Direction,
    other_strategy: T,
}

impl<T: Strategy> ConstantThenOtherStrategy<T> {
    fn new(first_direction: Direction, other_strategy: T) -> Self {
        Self {
            did_first_step: false,
            first_direction,
            other_strategy,
        }
    }
}

impl<T: Strategy> Strategy for ConstantThenOtherStrategy<T> {
    fn start(&mut self, game_info: &GameInfo) -> () {
        assert!(!self.did_first_step);
        self.other_strategy.start(game_info);
    }

    fn step(&mut self, board: &BoardTracker, time_budget: Duration) -> Direction {
        if self.did_first_step {
            self.other_strategy.step(board, time_budget)
        } else {
            self.did_first_step = true;
            self.first_direction
        }
    }
}

struct GetAwayFromItAllStrategy {
    player_id: usize,
}

impl GetAwayFromItAllStrategy {
    fn new() -> Self {
        Self { player_id: 0 }
    }
}

impl Strategy for GetAwayFromItAllStrategy {
    fn start(&mut self, game_info: &GameInfo) -> () {
        self.player_id = game_info.player_id as usize;
    }

    fn step(&mut self, board: &BoardTracker, _time_budget: Duration) -> Direction {
        let (width, _height) = board.board_size();
        let player_pos = board.get_player_latest_pos(self.player_id).unwrap();

        let occupied_mask = board.occupied_mask();
        let conservative_occupied_mask = board.conservative_occupied_mask(self.player_id);
        let distances = calculate_distances(board.board_size(), &occupied_mask);
        assert_eq!(distances.len(), occupied_mask.len());
        let reachable_mask = reachability::calculate_reachable(
            board.board_size(),
            &conservative_occupied_mask,
            player_pos,
        );
        assert_eq!(reachable_mask.len(), occupied_mask.len());

        let mut best_target: Option<((usize, usize), usize)> = None;
        for y in 0..width {
            for x in 0..width {
                let i = y * width + x;
                if (x, y) != player_pos
                    && reachable_mask[i]
                    && (best_target.is_none() || distances[i] > distances[best_target.unwrap().1])
                {
                    best_target = Some(((x, y), i));
                }
            }
        }
        if best_target.is_none() {
            println!("WARNING no best target found");

            let mut directions = Direction::all_directions().to_vec();
            directions.shuffle(&mut rand::thread_rng());

            for direction in directions {
                let new_player_pos = board.offset_pos(player_pos, direction);

                if board.get_cell_player(new_player_pos).is_none() {
                    return direction;
                }
            }

            println!("WARNING no way to survive");
            return Direction::Down;
        }
        let best_target = best_target.unwrap().0;

        let best_direction = shortest_path::shortest_path_next_direction(
            board.board_size(),
            &conservative_occupied_mask,
            player_pos,
            best_target,
        )
        .unwrap();

        best_direction
    }
}

struct PlayoutAfterNextStrategy {
    player_id: usize,
    max_steps: usize,
    win_multiplier: usize,
}

impl PlayoutAfterNextStrategy {
    fn new(max_steps: usize, win_multiplier: usize) -> Self {
        assert!(max_steps > 0);
        assert!(win_multiplier > 0);
        Self {
            player_id: 0,
            max_steps,
            win_multiplier,
        }
    }
}

impl Strategy for PlayoutAfterNextStrategy {
    fn start(&mut self, game_info: &GameInfo) -> () {
        self.player_id = game_info.player_id as usize;
    }

    fn step(&mut self, board: &BoardTracker, time_budget: Duration) -> Direction {
        let step_start = Instant::now();

        let n_players = board.count_seen();
        assert!(n_players > 0);
        assert!(self.player_id < n_players);

        let mut no_crash_directions: Vec<Direction> = Direction::all_directions()
            .iter()
            .filter(|&direction| {
                let new_player_pos = board.offset_pos(
                    board.get_player_latest_pos(self.player_id).unwrap(),
                    *direction,
                );
                board.get_cell_player(new_player_pos).is_none()
            })
            .cloned()
            .collect();
        no_crash_directions.shuffle(&mut rand::thread_rng());

        if no_crash_directions.is_empty() {
            println!("WARNING unavoidable crash");
            return Direction::Down;
        }
        if no_crash_directions.len() == 1 {
            return no_crash_directions[0];
        }

        #[derive(Clone, Debug)]
        struct DirectionStats {
            direction: Direction,
            score: f64,
            playouts: usize,
            mean_score: f64,
        }
        let mut stats_by_direction: Vec<_> = no_crash_directions
            .iter()
            .map(|&direction| DirectionStats {
                direction,
                score: 0.0,
                playouts: 0,
                mean_score: 0.0,
            })
            .collect();

        for i_playout in 0.. {
            if step_start.elapsed() > time_budget {
                break;
            }

            let own_playout_start_direction =
                no_crash_directions[i_playout % no_crash_directions.len()];

            let strategies_by_player: Vec<_> = (0..n_players)
                .map(|player_id| {
                    let fake_game_info = GameInfo {
                        width: 0,
                        height: 0,
                        player_id: player_id.try_into().unwrap(),
                    };
                    let base_strategy = NoCrashRandomStrategy::new();
                    let mut strategy: Box<dyn Strategy> = if player_id == self.player_id {
                        Box::new(ConstantThenOtherStrategy::new(
                            own_playout_start_direction,
                            base_strategy,
                        ))
                    } else {
                        Box::new(base_strategy)
                    };
                    strategy.start(&fake_game_info);
                    strategy
                })
                .collect();

            let playout_result = playout::run_playout(
                board.clone(),
                strategies_by_player,
                self.player_id,
                self.max_steps,
            );
            // let mut playout_score = playout_result.beaten_players;
            // if playout_result.did_win {
            //     playout_score *= self.win_multiplier;
            // }

            // let playout_score = playout_result.survived_steps;

            let playout_score: f64 = if playout_result.did_win {
                1.0
            } else if playout_result.did_die {
                0.0
            } else {
                assert!(playout_result.remaining_players > 0);
                1.0 / (playout_result.remaining_players as f64)
            };

            // let playout_score = (playout_result.beaten_players - board.count_dead()) as f64;

            let stats = &mut stats_by_direction[i_playout % no_crash_directions.len()];
            stats.score += playout_score;
            stats.playouts += 1;
        }

        for stats in stats_by_direction.iter_mut() {
            if stats.playouts > 0 {
                stats.mean_score = stats.score as f64 / stats.playouts as f64;
            }
            println!("{:?}", stats);
        }
        let best_direction = stats_by_direction
            .iter()
            .max_by(|a, b| a.mean_score.partial_cmp(&b.mean_score).unwrap())
            .unwrap()
            .direction;
        best_direction
    }
}

fn run_round<S: Strategy, R: Read, W: Write>(
    mut strategy: S,
    reader: &mut GameReader<R>,
    writer: &mut GameWriter<W>,
) -> Result<()> {
    println!("waiting for next round");

    let (mut board, mut last_tick_at) = loop {
        let msg = reader.read()?;
        println!("{:?}", msg);

        match msg {
            ServerMessage::Game { message } => {
                strategy.start(&message);
                let board = BoardTracker::new(
                    message.width.try_into().unwrap(),
                    message.height.try_into().unwrap(),
                );
                break (board, Instant::now());
            }
            ServerMessage::Error { message } => {
                return Err(anyhow!("error: {}", message));
            }
            ServerMessage::Motd { .. } => (),
            ServerMessage::Win { .. } => return Ok(()),
            ServerMessage::Lose { .. } => return Ok(()),
            _ => (),
        };
    };

    loop {
        let msg = reader.read()?;
        println!("{:?}", msg);

        match msg {
            ServerMessage::Tick => {
                let tick_duration = last_tick_at.elapsed();
                last_tick_at = Instant::now();
                let time_budget =
                    Duration::saturating_sub(tick_duration / 3, Duration::from_millis(10));
                let before_step = Instant::now();
                let direction = strategy.step(&board, time_budget);
                let step_duration = before_step.elapsed();
                println!(
                    "--- moving {} ({} ms calc, {} ms budget, {} ms since last tick) ---\n",
                    direction,
                    step_duration.as_millis(),
                    time_budget.as_millis(),
                    tick_duration.as_millis()
                );
                writer.write(&ClientMessage::Move { direction })?;
            }
            ServerMessage::Game { .. } => (),
            ServerMessage::Motd { .. } => (),
            ServerMessage::Error { message } => {
                return Err(anyhow!("error: {}", message));
            }
            ServerMessage::Pos { player_id, x, y } => {
                board.record_pos(
                    player_id.try_into().unwrap(),
                    (x.try_into().unwrap(), y.try_into().unwrap()),
                );
            }
            ServerMessage::Player { .. } => (),
            ServerMessage::Die { player_ids } => {
                for player_id in player_ids {
                    board.record_death(player_id.try_into().unwrap());
                }
            }
            ServerMessage::Message { .. } => (),
            ServerMessage::Win { .. } => return Ok(()),
            ServerMessage::Lose { .. } => return Ok(()),
        };
    }
}

fn try_play(host_port: String, username: String, password: String) -> Result<()> {
    println!("connecting");

    let stream = TcpStream::connect(host_port)?;
    let mut reader = GameReader::new(&stream);
    let mut writer = GameWriter::new(&stream);

    writer.write(&ClientMessage::Join { username, password })?;

    let strategy = PlayoutAfterNextStrategy::new(50, 1);
    run_round(strategy, &mut reader, &mut writer)?;

    Ok(())
}

fn run_online() -> Result<()> {
    let host_port =
        std::env::var("GPN_SNAKE_HOST_PORT").unwrap_or("gpn-tron.duckdns.org:4000".to_string());
    let username = std::env::var("GPN_SNAKE_USERNAME").expect("GPN_SNAKE_USERNAME is not set");
    let password = std::env::var("GPN_SNAKE_PASSWORD").expect("GPN_SNAKE_PASSWORD is not set");

    loop {
        println!(
            "restarting due to error: {:?}",
            try_play(host_port.clone(), username.clone(), password.clone(),),
        );
        std::thread::sleep(time::Duration::from_millis(50));
    }
}

fn main() -> Result<()> {
    run_online()?;

    Ok(())
}
