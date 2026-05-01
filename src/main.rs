#![feature(macro_metavar_expr_concat)]

mod connect;
mod game;
mod rcolumn;
mod settings;

use connect::Connect;
use game::Game;
use iced::{
    Font, Length, Theme,
    advanced::widget::Text,
    keyboard::{self, Key},
    widget::{Row, button, column},
    window,
};
use settings::Settings;

type Task<T = Message> = iced::Task<T>;
type Subscription<T = Message> = iced::Subscription<T>;
type Element<'a, M = Message> = iced::Element<'a, M, Theme, iced::Renderer>;

#[derive(Clone)]
enum Message {
    Game(game::Message),
    Connect(connect::Message),
    Screen(Screen),
    Exit,
    ToggleFullscreen,
    ToggleDarkMode(bool),
    ToggleDebugMode,
}

#[allow(clippy::struct_excessive_bools)]
struct App {
    debug_mode: bool,
    text: f32,
    screen: Screen,
    game: Game,
    connect: Connect,
    settings: &'static Settings,
}

pub const TITLE: &str = concat!("Shogui ", env!("CARGO_PKG_VERSION"));

fn main() -> iced::Result {
    let settings = Box::leak(Box::new(Settings::read().unwrap()));
    iced::application(|| App::new(settings), App::update, App::view)
        .title(TITLE)
        .subscription(App::subscription)
        .theme(App::theme)
        .default_font(Font::MONOSPACE)
        .run()
}

impl App {
    fn new(settings: &'static Settings) -> Self {
        Self {
            game: Game::default(),
            connect: Connect::new(settings),
            debug_mode: false,
            text: 16.0,
            screen: Screen::Game,
            settings,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Screen {
    Game,
    Connect,
}

impl App {
    #[allow(clippy::needless_pass_by_value)]
    fn update(&mut self, message: Message) -> Task {
        match message {
            Message::Exit => std::process::exit(0),
            Message::ToggleFullscreen => return toggle_fullscreen(),
            Message::ToggleDarkMode(state) => _ = self.settings.set_dark_mode(state),
            Message::ToggleDebugMode => self.debug_mode = !self.debug_mode,
            Message::Screen(screen) => self.screen = screen,
            Message::Game(message) => return self.game.update(message, &mut self.connect),
            Message::Connect(message) => {
                if let connect::Message::Connected(..) = message {
                    self.screen = Screen::Game;
                }
                return self.connect.update(message, &mut self.game);
            }
        }
        Task::none()
    }

    fn view(&self) -> Element<'_> {
        self.explain(column![self.screens(), self.screen_view()].spacing(8.0))
    }

    fn screens(&self) -> Element<'_> {
        let screens = [Screen::Game, Screen::Connect].iter().zip(["Game", "Connect"]);
        Element::new(
            Row::with_children(screens.map(|(screen, name)| {
                Element::new(button(Text::new(name).center()).on_press(screen).width(Length::Fill))
            }))
            .spacing(4.0),
        )
        .map(|screen| Message::Screen(*screen))
    }

    fn screen_view(&self) -> Element<'_> {
        match self.screen {
            Screen::Game => self.game.view(self).map(Message::Game),
            Screen::Connect => self.connect.view(self).map(Message::Connect),
        }
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
        if self.settings.dark_mode() { Theme::Dark } else { Theme::Light }
    }
    #[allow(clippy::unused_self)]
    fn subscription(&self) -> Subscription {
        Subscription::batch([keyboard::listen().filter_map(|event| {
            let keyboard::Event::KeyPressed { key, modifiers, .. } = event else { return None };
            Some(if key == Key::Character("q".into()) && modifiers.alt() {
                Message::Exit
            } else if key == Key::Character("f".into()) {
                Message::ToggleFullscreen
            } else if key == Key::Character("d".into()) && modifiers.control() {
                Message::ToggleDebugMode
            } else {
                return None;
            })
        })])
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
