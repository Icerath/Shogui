mod board;

use std::sync::OnceLock;

use board::{BoardState, BoardStateEvent};
use iced::{
    Font, Length, Padding, Theme,
    advanced::svg,
    keyboard::{self, Key},
    widget::{Text, Toggler, button, column, container::Container, row},
    window,
};
use petty_shogi::Piece;

type Task<T = Message> = iced::Task<T>;
type Subscription<T = Message> = iced::Subscription<T>;
type Element<'a> = iced::Element<'a, Message, Theme, iced::Renderer>;

const DEFAULT_DARK_MODE: bool = true;

#[derive(Debug, Clone)]
enum Message {
    Exit,
    ToggleFullscreen,
    ResetBoard,
    ToggleDarkMode(bool),
    ToggleDebugMode(bool),
    BoardStateEvent(BoardStateEvent),
}

#[allow(clippy::struct_excessive_bools)]
struct App {
    board_state: BoardState,
    darkmode: bool,
    debug_mode: bool,
}

fn main() -> iced::Result {
    iced::application(App::default, App::update, App::view)
        .title("Shogui")
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(Font::MONOSPACE)
        .run()
}

impl Default for App {
    fn default() -> Self {
        Self { board_state: BoardState::init(), darkmode: DEFAULT_DARK_MODE, debug_mode: false }
    }
}

impl App {
    #[allow(clippy::needless_pass_by_value)]
    fn update(&mut self, message: Message) -> Task {
        match message {
            Message::Exit => std::process::exit(0),
            Message::ToggleFullscreen => return toggle_fullscreen(),
            Message::ResetBoard => self.board_state = BoardState::init(),
            Message::ToggleDarkMode(state) => self.darkmode = state,
            Message::ToggleDebugMode(state) => self.debug_mode = state,
            Message::BoardStateEvent(event) => self.board_state.update(event),
        }
        Task::none()
    }

    fn view(&self) -> Element<'_> {
        self.explain(row![self.ui(), Container::new(self.board()).padding(8)])
    }

    fn explain<'a>(&self, el: impl Into<Element<'a>>) -> Element<'a> {
        let el = el.into();
        let explain_color = self.theme().palette().text;
        if self.debug_mode {
            return el.explain(explain_color);
        }
        el
    }

    fn theme(&self) -> Theme {
        if self.darkmode { Theme::Dark } else { Theme::Light }
    }
    #[allow(clippy::unused_self)]
    fn subscription(&self) -> Subscription {
        Subscription::batch([keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else { return None };
            Some(if key == Key::Character("q".into()) && modifiers.alt() {
                Message::Exit
            } else if key == Key::Character("f".into()) && modifiers.control() {
                Message::ToggleFullscreen
            } else if key == Key::Character("f".into()) {
                Message::BoardStateEvent(BoardStateEvent::FlipBoard)
            } else {
                return None;
            })
        })])
    }
}

impl App {
    fn piece_svg(piece: Piece) -> svg::Svg {
        static SVGS: OnceLock<[svg::Svg; Piece::LEN]> = OnceLock::new();
        let svgs = SVGS.get_or_init(|| {
            Piece::ALL.map(|piece| {
                let kind = format!("{:?}", piece.kind()).to_lowercase();
                let promoted = if piece.promoted() { "promoted-" } else { "" };
                svg::Svg::new(format!(
                    "{}/assets/pieces/{promoted}{kind}.svg",
                    env!("CARGO_MANIFEST_DIR")
                ))
            })
        });
        svgs[piece].clone()
    }
    fn board(&self) -> Element<'_> {
        Container::new(&self.board_state)
            .width(Length::Fill)
            .height(Length::Fill)
            .center(Length::Fill)
            .into()
    }
    fn ui(&self) -> Element<'_> {
        column![]
            .push(button("Reset Board").on_press(Message::ResetBoard))
            .push(Toggler::new(self.darkmode).label("Dark Mode").on_toggle(Message::ToggleDarkMode))
            .push(
                Toggler::new(self.debug_mode)
                    .label("Debug Mode")
                    .on_toggle(Message::ToggleDebugMode),
            )
            .push(Text::new(format!("Moves: {}", self.board_state.legal_moves.len())))
            .padding(Padding::from(8))
            .spacing(8.0)
            .width(Length::Shrink)
            .into()
    }
}

fn toggle_fullscreen() -> Task {
    window::latest().and_then(|id| {
        window::mode(id).map(toggle_mode).then(move |mode| window::set_mode(id, mode))
    })
}

fn toggle_mode(mode: window::Mode) -> window::Mode {
    match mode {
        window::Mode::Fullscreen => window::Mode::Windowed,
        window::Mode::Windowed => window::Mode::Fullscreen,
        hidden @ window::Mode::Hidden => hidden,
    }
}
