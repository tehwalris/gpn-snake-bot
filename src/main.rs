#![allow(dead_code)]

use anyhow::anyhow;
use anyhow::Result;
use core::time;
use direction::Direction;
use rand::prelude::SliceRandom;
use std::{
    io::{BufRead, BufReader, BufWriter, Read, Write},
    net::TcpStream,
};

mod board_tracker;
mod direction;

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

trait Strategy: Sized {
    fn start(&mut self, game_info: &GameInfo) -> ();
    fn step(&mut self) -> Result<Direction>;
    fn record_pos(&mut self, player_id: usize, pos: (usize, usize));
    fn record_death(&mut self, player_id: usize);
}

struct AlwaysDownStrategy {}

impl AlwaysDownStrategy {
    fn new() -> Self {
        Self {}
    }
}

impl Strategy for AlwaysDownStrategy {
    fn start(&mut self, _game_info: &GameInfo) -> () {}

    fn step(&mut self) -> Result<Direction> {
        Ok(Direction::Down)
    }

    fn record_pos(&mut self, _player_id: usize, _pos: (usize, usize)) {}

    fn record_death(&mut self, _player_id: usize) {}
}

struct NoCrashRandomStrategy {
    board: Option<board_tracker::BoardTracker>,
    player_id: usize,
}

impl NoCrashRandomStrategy {
    fn new() -> Self {
        Self {
            board: None,
            player_id: 0,
        }
    }
}

impl Strategy for NoCrashRandomStrategy {
    fn start(&mut self, game_info: &GameInfo) -> () {
        self.board = Some(board_tracker::BoardTracker::new(
            game_info.width as usize,
            game_info.height as usize,
        ));
        self.player_id = game_info.player_id as usize;
    }

    fn step(&mut self) -> Result<Direction> {
        let board = self.board.as_mut().unwrap();
        let player_pos = board.get_player_latest_pos(self.player_id).unwrap();

        let mut directions = vec![
            Direction::Up,
            Direction::Right,
            Direction::Down,
            Direction::Left,
        ];
        directions.shuffle(&mut rand::thread_rng());

        for direction in directions {
            let new_player_pos = board.offset_pos(player_pos, direction);

            if board.get_cell_player(new_player_pos).is_none() {
                return Ok(direction);
            }
        }

        Ok(Direction::Down)
    }

    fn record_pos(&mut self, player_id: usize, pos: (usize, usize)) {
        self.board.as_mut().unwrap().record_pos(player_id, pos);
    }

    fn record_death(&mut self, player_id: usize) {
        self.board.as_mut().unwrap().record_death(player_id);
    }
}

fn run_round<S: Strategy, R: Read, W: Write>(
    mut strategy: S,
    reader: &mut GameReader<R>,
    writer: &mut GameWriter<W>,
) -> Result<()> {
    println!("waiting for next round");

    loop {
        let msg = reader.read()?;
        println!("{:?}", msg);

        match msg {
            ServerMessage::Game { message } => {
                strategy.start(&message);
                break;
            }
            ServerMessage::Error { .. } => return Ok(()),
            ServerMessage::Motd { .. } => (),
            ServerMessage::Win { .. } => return Ok(()),
            ServerMessage::Lose { .. } => return Ok(()),
            _ => (),
        };
    }

    loop {
        let msg = reader.read()?;
        println!("{:?}", msg);

        match msg {
            ServerMessage::Tick => {
                let direction = strategy.step()?;
                println!("moving {}", direction);
                writer.write(&ClientMessage::Move { direction })?;
            }
            ServerMessage::Game { .. } => (),
            ServerMessage::Motd { .. } => (),
            ServerMessage::Error { .. } => return Ok(()),
            ServerMessage::Pos { player_id, x, y } => strategy.record_pos(
                player_id.try_into().unwrap(),
                (x.try_into().unwrap(), y.try_into().unwrap()),
            ),
            ServerMessage::Player { .. } => (),
            ServerMessage::Die { player_ids } => {
                for player_id in player_ids {
                    strategy.record_death(player_id.try_into().unwrap());
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

    run_round(NoCrashRandomStrategy::new(), &mut reader, &mut writer)?;

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
        std::thread::sleep(time::Duration::from_millis(200));
    }
}

fn main() -> Result<()> {
    run_online()?;

    Ok(())
}
