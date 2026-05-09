mod protocol;

use std::{
    io,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};

pub use protocol::Packet;
use protocol::encode_invite;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;

use iced::{
    Background, Border, Length, Renderer, Theme,
    advanced::graphics::futures::MaybeSend,
    clipboard,
    futures::channel::oneshot,
    widget::{self, Button, Container, Radio, Text, TextInput, button, column, row},
};
use iced_aw::Spinner;
use petty_shogi::{Board, Side};

use crate::{
    App, Task,
    game::{self, Game, board::BoardState},
    settings::Settings,
};

type Element<'a, T = Message> = crate::Element<'a, T>;

pub struct Connect {
    port: String,
    invite: String,
    state: State,
    pub game_settings: GameSettings,
    settings: &'static Settings,
}

impl Connect {
    pub fn new(settings: &'static Settings) -> Self {
        Self {
            port: settings.port().map_or_else(String::new, |port| port.to_string()),
            invite: String::default(),
            state: State::default(),
            game_settings: GameSettings::default(),
            settings,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    OpenHost,
    SubmitHost,
    CopyInvite,
    SetPort(String),
    SetHostSide(Side),
    OpenJoin,
    SubmitJoin,
    SetInvite(String),
    Connected(Arc<TcpStream>, SocketAddr, Option<GameSettings>),
    CloseConnection,
    LocalError(Arc<io::Error>),
    RemoteError(Arc<io::Error>),
    Recv(Packet),
    Send(Packet),
    None,
}

#[derive(Default)]
pub enum State {
    #[default]
    JoinMenu,
    HostMenu,
    PendingHost {
        _cancel: oneshot::Receiver<()>,
        ip: IpAddr,
        port: u16,
        key: u64,
    },
    PendingJoin,
    Connected {
        _cancel: oneshot::Receiver<()>,
        host: bool,
        stream: Arc<TcpStream>,
        _addr: SocketAddr,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GameSettings {
    pub host_side: Side,
    pub eval_bar: bool,
    pub start_position: String,
}
impl Default for GameSettings {
    fn default() -> Self {
        Self {
            host_side: Side::Sente,
            eval_bar: false,
            start_position: Board::start_pos().to_sfen(),
        }
    }
}

fn task(future: impl Future<Output = Message> + MaybeSend + 'static) -> Task {
    Task::future(future).map(crate::Message::Connect)
}
impl Connect {
    pub fn our_side(&self) -> Option<Side> {
        let State::Connected { host, .. } = self.state else { return None };
        Some(if host { self.game_settings.host_side } else { !self.game_settings.host_side })
    }
    pub fn is_connected(&self) -> bool {
        self.our_side().is_some()
    }
    pub fn update(&mut self, message: Message, game: &mut Game) -> Task {
        match message {
            Message::OpenHost => self.state = State::HostMenu,
            Message::OpenJoin => self.state = State::JoinMenu,
            Message::SubmitHost => {
                let port = if self.port.trim().is_empty() {
                    self.settings.port()
                } else {
                    self.port.parse::<u16>().ok()
                };
                _ = self.settings.set_port(port);
                let Some(port) = port else { return Task::none() };
                let Ok(ip) = local_ip_address::local_ip() else {
                    // TODO: show error
                    return Task::none();
                };
                let key = rand::random::<u64>();
                let (tx, rx) = oneshot::channel();
                self.state = State::PendingHost { _cancel: rx, ip, port, key };
                return task(protocol::host(tx, ip, port, key, self.game_settings.clone()));
            }
            Message::SetPort(port) => self.port = port,
            Message::SetInvite(invite) => self.invite = invite,
            Message::SetHostSide(side) => self.game_settings.host_side = side,
            Message::SubmitJoin => {
                let Some((ip, port, key)) = protocol::decode_invite(self.invite.as_bytes()) else {
                    eprintln!("invalid invite");
                    // TODO: invalid invite
                    return Task::none();
                };
                self.state = State::PendingJoin;
                return task(protocol::join(ip, port, key));
            }
            Message::LocalError(error) => {
                eprintln!("local error: {error:?}");
                if let State::PendingJoin = self.state {
                    self.state = State::JoinMenu;
                }
            }
            Message::RemoteError(error) => {
                eprintln!("remote error: {error:?}");
                if let State::PendingJoin = self.state {
                    self.state = State::JoinMenu;
                }
            }
            Message::CopyInvite => {
                let State::PendingHost { ip, port, key, .. } = self.state else {
                    return Task::none();
                };
                return clipboard::write(encode_invite(ip, port, key));
            }
            Message::Connected(stream, addr, settings) => {
                let host = matches!(self.state, State::PendingHost { .. });
                let (tx, rx) = oneshot::channel();
                self.state =
                    State::Connected { host, stream: stream.clone(), _addr: addr, _cancel: rx };

                if let Some(settings) = settings {
                    self.game_settings = settings;
                }

                let board = Board::from_sfen(&self.game_settings.start_position)
                    .expect("Sfen be validated");
                game.board_state = BoardState::init(board);
                let our_side = self.our_side().unwrap();

                game.board_state.playing = Some(our_side);
                game.board_state.face_up = our_side;

                return Task::stream(iced::stream::channel(4, move |sender| {
                    protocol::recv(stream.clone(), sender, tx)
                }))
                .map(crate::Message::Connect);
            }
            Message::Recv(packet) => {
                let &State::Connected { host, ref stream, .. } = &self.state else {
                    return Task::none();
                };
                match packet {
                    Packet::CloseConnection | Packet::Rejected => {
                        self.state = if host { State::HostMenu } else { State::JoinMenu };
                    }
                    Packet::PlayMove(mov) => {
                        if !game.board_state.board.is_legal(mov) {
                            return task(protocol::send(stream.clone(), Packet::CloseConnection));
                        }

                        return game
                            .update(game::Message::Board(game::board::Message::Move(mov)), self);
                    }
                }
                return Task::none();
            }
            Message::Send(packet) => {
                let State::Connected { stream, .. } = &self.state else { return Task::none() };
                return task(protocol::send(stream.clone(), packet));
            }
            Message::CloseConnection => {
                game.board_state.playing = None;
                let State::Connected { host, stream, .. } = &self.state else {
                    return Task::none();
                };
                let task = task(protocol::send(stream.clone(), Packet::CloseConnection));
                self.state = if *host { State::HostMenu } else { State::JoinMenu };
                return task;
            }
            Message::None => {}
        }
        Task::none()
    }
    pub fn view(&self, app: &App) -> Element<'_> {
        Container::new(
            Container::new(
                column![
                    self.menu(),
                    Container::new(match self.state {
                        State::HostMenu => self.host_menu(app.text),
                        State::JoinMenu => self.join_menu(app.text),
                        State::PendingHost { .. } => Self::pending_host(app.text),
                        State::PendingJoin => Self::pending_join(app.text),
                        State::Connected { .. } =>
                            cancel_button(Text::new("Close Connection").size(app.text))
                                .on_press(Message::CloseConnection)
                                .into(),
                    })
                    .center_x(Length::Fill)
                ]
                .spacing(8.0),
            )
            .padding(8.0)
            .style(|theme: &Theme| widget::container::Style {
                background: Some(Background::Color(theme.palette().background)),
                border: Border::default().width(1.0).color(theme.palette().text),
                ..Default::default()
            })
            .width(Length::Fixed(500.0))
            .height(Length::Fixed(200.0)),
        )
        .center(Length::Fill)
        .into()
    }

    fn menu(&self) -> Element<'_> {
        let mut host = button(Text::new("Host").center()).width(Length::Fill);
        let mut join = button(Text::new("Join").center()).width(Length::Fill);

        match self.state {
            State::HostMenu => join = join.on_press(Message::OpenJoin),
            State::JoinMenu => host = host.on_press(Message::OpenHost),
            _ => {}
        }
        row![host, join,].spacing(4.0).width(Length::Fill).into()
    }

    fn host_menu(&self, text_size: f32) -> Element<'_> {
        column![
            row![
                Text::new("Host Side:"),
                Radio::new(
                    "Sente",
                    Side::Sente,
                    Some(self.game_settings.host_side),
                    Message::SetHostSide
                ),
                Radio::new(
                    "Gote",
                    Side::Gote,
                    Some(self.game_settings.host_side),
                    Message::SetHostSide
                )
            ],
            row![
                Text::new("Port: ").size(text_size),
                TextInput::new("3000", &self.port)
                    .size(text_size)
                    .on_input(Message::SetPort)
                    .width(Length::Fixed(text_size * 4.0)),
            ],
            button("Submit").on_press(Message::SubmitHost),
        ]
        .into()
    }

    fn join_menu(&self, text_size: f32) -> Element<'_> {
        column![
            row![
                Text::new("Invite: ").size(text_size),
                TextInput::new("", &self.invite)
                    .size(text_size)
                    .on_input(Message::SetInvite)
                    .on_submit(Message::SubmitJoin),
            ],
            button("Submit").on_press(Message::SubmitJoin),
        ]
        .into()
    }

    fn pending_host(text_size: f32) -> Element<'static> {
        column![
            row![
                cancel_button(Text::new("Cancel").size(text_size)).on_press(Message::OpenHost),
                Container::new(Spinner::new()),
            ]
            .height(Length::Shrink)
            .spacing(8.0),
            button(Text::new("Copy Invite").size(text_size)).on_press(Message::CopyInvite)
        ]
        .spacing(4.0)
        .into()
    }

    fn pending_join(text_size: f32) -> Element<'static> {
        column![
            row![
                cancel_button(Text::new("Cancel").size(text_size)).on_press(Message::OpenJoin),
                Container::new(Spinner::new()),
            ]
            .height(Length::Shrink)
            .spacing(8.0)
        ]
        .spacing(4.0)
        .into()
    }
}

fn cancel_button<'a>(inner: impl Into<Element<'a>>) -> Button<'a, Message, Theme, Renderer> {
    button(inner).style(|theme: &Theme, _| {
        let pair = theme.extended_palette().danger.weak;
        widget::button::Style {
            text_color: pair.text,
            background: Some(iced::Background::Color(pair.color)),
            ..Default::default()
        }
    })
}
