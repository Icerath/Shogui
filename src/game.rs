pub mod board;

use board::BoardState;
use iced::{
    Length,
    widget::{Column, Container, Space, Text, button, row, text_input},
};
use petty_shogi::Board;

use crate::{
    App,
    connect::{self, Connect, Packet},
    rcolumn::RColumn,
};

type Element<'a, T = Message> = crate::Element<'a, T>;

#[derive(Debug, Clone)]
pub enum Message {
    Reset,
    SetSfen(String),
    Board(board::Message),
}

pub struct Game {
    pub board_state: BoardState,
    pub sfen: String,
}

impl Default for Game {
    fn default() -> Self {
        let board = Board::start_pos();
        Self { sfen: board.to_sfen(), board_state: BoardState::init(board) }
    }
}

impl Game {
    pub fn update(&mut self, message: Message, connect: &mut Connect) -> crate::Task {
        match message {
            Message::Reset => {
                self.board_state = BoardState::init(Board::start_pos());
                self.sfen = self.board_state.board.to_sfen();
            }
            Message::Board(message) => {
                self.board_state.update(message);
                self.sfen = self.board_state.board.to_sfen();
                if let Some(our_side) = connect.our_side()
                    && let board::Message::Move(mov) = message
                    && self.board_state.board.active != our_side
                {
                    return connect.update(connect::Message::Send(Packet::PlayMove(mov)), self);
                }
            }
            Message::SetSfen(sfen) => {
                if let Some(board) = Board::from_sfen(&sfen) {
                    self.board_state = BoardState::init(board);
                    self.sfen = self.board_state.board.to_sfen();
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
            column = column.push(button(Text::new("Reset Board")).on_press(Message::Reset));
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
                        .on_input(Message::SetSfen)
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
