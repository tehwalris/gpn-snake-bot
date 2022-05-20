use anyhow::anyhow;
use anyhow::Result;
use core::time;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::fmt;
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
        if !pos.top {
            return Ok(Direction::Up);
        } else if !pos.right {
            return Ok(Direction::Right);
        } else if !pos.bottom {
            return Ok(Direction::Down);
        } else if !pos.left {
            return Ok(Direction::Left);
        } else {
            return Err(anyhow!("no valid direction to move"));
        }
    }
}

struct RandomPossibleStrategy {}

impl Strategy for RandomPossibleStrategy {
    fn start(_goal: (i32, i32)) -> Self {
        Self {}
    }

    fn step(&mut self, pos: PosWithWalls) -> Result<Direction> {
        let mut possible_dirs = Vec::new();
        if !pos.top {
            possible_dirs.push(Direction::Up);
        }
        if !pos.right {
            possible_dirs.push(Direction::Right);
        }
        if !pos.bottom {
            possible_dirs.push(Direction::Down);
        }
        if !pos.left {
            possible_dirs.push(Direction::Left);
        }

        if let Some(&dir) = possible_dirs.choose(&mut thread_rng()) {
            Ok(dir)
        } else {
            Err(anyhow!("no valid direction to move"))
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

fn main() -> Result<()> {
    let username = std::env::var("GPN_MAZING_USERNAME").expect("GPN_MAZING_USERNAME is not set");
    let password = std::env::var("GPN_MAZING_PASSWORD").expect("GPN_MAZING_PASSWORD is not set");

    let stream = TcpStream::connect("94.45.241.27:4000")?;
    let mut reader = GameReader::new(&stream);
    let mut writer = GameWriter::new(&stream);

    writer.write(&ClientMessage::Join { username, password })?;

    loop {
        run_round::<_, _, RandomPossibleStrategy>(&mut reader, &mut writer)?;
    }
}
