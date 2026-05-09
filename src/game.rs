pub mod board;

use std::sync::Arc;

use board::BoardState;
use iced::{
    Length,
    futures::StreamExt as _,
    widget::{Column, Container, Space, Text, button, row, text_input},
};
use petty_shogi::{
    Board, Engine, Move,
    command::{Command, GoCommand, Position},
    response::{BestMove, Response},
};

use crate::{
    App,
    connect::{self, Connect, Packet},
    rcolumn::RColumn,
};

type Element<'a, T = Message> = crate::Element<'a, T>;

#[derive(Debug, Clone)]
pub enum Message {
    PlayEngine,
    Reset,
    SetSfen(String),
    SubmitSfen,
    Board(board::Message),
    EngineResponse(Arc<Response>),
    SetPly(usize),
}

pub struct Game {
    pub history: Vec<(Board, Move)>,
    pub board_state: BoardState,
    pub sfen: String,
    pub engine: Option<Engine>,
}

impl Default for Game {
    fn default() -> Self {
        let board = Board::start_pos();
        Self {
            sfen: board.to_sfen(),
            board_state: BoardState::init(board),
            engine: None,
            history: vec![],
        }
    }
}

impl Game {
    pub fn update(&mut self, message: Message, connect: &mut Connect) -> crate::Task {
        match message {
            Message::SetPly(ply) => 'blk: {
                let Some((board, _mov)) = self.history.get(ply).cloned() else { break 'blk };
                self.history.truncate(ply);
                self.sfen = board.to_sfen();
                let playing = self.board_state.playing;
                self.board_state = BoardState::init(board);
                self.board_state.playing = playing;
                self.board_state.last_move = ply.checked_sub(1).map(|ply| self.history[ply].1);
                if let Some(engine) = &self.engine {
                    engine.stop();
                }
            }
            Message::EngineResponse(response) => match *response {
                Response::BestMove(BestMove::Move { mov, ponder: _ }) => {
                    if !self.board_state.board.is_legal(mov) {
                        eprintln!("[ERROR] engine tried to play {mov}");
                        return crate::Task::none();
                    }
                    self.history.push((self.board_state.board.clone(), mov));
                    self.board_state.update(board::Message::Move(mov));
                    self.sfen = self.board_state.board.to_sfen();
                    self.engine
                        .as_mut()
                        .unwrap()
                        .position(Position::Sfen(self.sfen.clone()), vec![]);
                }
                _ => eprintln!("{response}"),
            },
            Message::PlayEngine => {
                let (tx, rx) = iced::futures::channel::mpsc::unbounded();
                let mut engine = Engine::default();
                engine.set_recv(move |response| _ = tx.unbounded_send(response));
                self.engine = Some(engine);
                self.board_state.playing = Some(self.board_state.board.active);

                return iced::Task::stream(
                    rx.map(|response| Message::EngineResponse(Arc::new(response))),
                )
                .map(crate::Message::Game);
            }
            Message::Reset => {
                self.board_state = BoardState::init(Board::start_pos());
                self.sfen = self.board_state.board.to_sfen();
                self.history.clear();
            }
            Message::Board(message) => {
                if let board::Message::Move(mov) = message {
                    self.history.push((self.board_state.board.clone(), mov));
                }
                self.board_state.update(message);
                self.sfen = self.board_state.board.to_sfen();

                if let Some(our_side) = self.board_state.playing
                    && let board::Message::Move(mov) = message
                    && self.board_state.board.active != our_side
                {
                    if connect.is_connected() {
                        return connect.update(connect::Message::Send(Packet::PlayMove(mov)), self);
                    }
                    if let Some(engine) = &mut self.engine {
                        engine.process_command(Command::Position(
                            Position::Sfen(self.sfen.clone()),
                            vec![],
                        ));
                        engine.process_command(Command::Go(GoCommand {
                            movetime: Some(1000),
                            ..GoCommand::default()
                        }));
                    }
                }
            }
            Message::SetSfen(sfen) => self.sfen = sfen,
            Message::SubmitSfen => {
                if let Some(board) = Board::from_sfen(&self.sfen) {
                    self.board_state = BoardState::init(board);
                }
            }
        }
        crate::Task::none()
    }
    pub fn view(&self, _: &App) -> Element<'_> {
        row![self.ui(), self.board()].into()
    }

    fn ui(&self) -> Element<'_> {
        let mut column = Column::new();
        if self.board_state.playing.is_none() {
            column = column
                .push(button("Reset Board").on_press(Message::Reset))
                .push(button("Play Engine").on_press(Message::PlayEngine));
        }
        column
            .push(Text::new(format!("Moves: {}", self.board_state.legal_moves.len())))
            .padding(8.0)
            .spacing(8.0)
            .into()
    }

    fn board(&self) -> Element<'_> {
        Container::new(
            RColumn::new([
                Element::<board::Message>::new(&self.board_state).map(Message::Board),
                row![
                    Container::new(Text::new("SFEN").size(16.0).center())
                        .center_x(Length::Fill)
                        .padding(4.0),
                    text_input("", &self.sfen)
                        .on_input_maybe(
                            self.board_state.playing.is_none().then_some(Message::SetSfen)
                        )
                        .on_submit_maybe(
                            self.board_state.playing.is_none().then_some(Message::SubmitSfen)
                        )
                        .width(Length::FillPortion(9)),
                    Space::new().width(Length::Fill)
                ]
                .into(),
            ])
            .spacing(4.0),
        )
        .center(Length::Fill)
        .padding(8)
        .into()
    }
}
