mod draw;

use iced::{
    Background, Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
    advanced::{
        Layout, Widget,
        image::{self, Image, Renderer as _},
        layout::{self, Limits},
        renderer::{Quad, Renderer as _},
        svg::Renderer as _,
        widget::{self},
    },
    mouse::{self, Cursor},
};
use petty_shogi::{Bitboard, Board, File, Move, Piece, PieceKind, Rank, Side, Square};

const LAST_MOVE_COLOR: Color = Color { r: 0.6, g: 0.8, b: 0.2, a: 0.5 };
const MOVE_OPTION_COLOR: Color = Color { r: 0.4, b: 0.4, g: 0.9, a: 0.5 };
const CHECK_COLOR: Color = Color { r: 0.9, g: 0.4, b: 0.4, a: 0.5 };

pub struct BoardState {
    pub board: Board,
    pub legal_moves: Vec<Move>,
    pub selected: Option<SelectedPiece>,
    pub move_options: Bitboard,

    pub board_image: Image,
    pub last_move: Option<Move>,
    pub promote_state: Option<PromoteState>,
    pub under_check: Option<Square>,
    pub face_up: Side,
    pub playing: Option<Side>,
}

pub struct PromoteState {
    from: Square,
    to: Square,
    nonpromote: Square,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    Move(Move),
    Selected(Option<SelectedPiece>),
    Promote(Square, Square),
    FlipBoard,
}

impl BoardState {
    pub fn init(board: Board) -> Self {
        Self {
            legal_moves: board.legal_moves(vec![]),
            board,
            selected: None,
            move_options: Bitboard::EMPTY,
            board_image: Image::new(format!("{}/assets/board.png", env!("CARGO_MANIFEST_DIR"))),
            last_move: None,
            promote_state: None,
            under_check: None,
            face_up: Side::Sente,
            playing: None,
        }
    }
    pub fn update(&mut self, event: Message) {
        match event {
            Message::Selected(selection) => {
                self.selected = selection;
                self.promote_state = None;
                self.move_options = Bitboard::EMPTY;
                let Some(selected) = selection else { return };
                if self.selected_side(selected) != self.board.active {
                    return;
                }
                if let Some(playing) = self.playing
                    && self.selected_side(selected) != playing
                {
                    return;
                }
                self.update_move_options(selected);
            }
            Message::Promote(from, to) => {
                self.selected = None;
                self.move_options = Bitboard::EMPTY;

                let nonpromote = to
                    .back(self.board.active)
                    .unwrap_or_else(|| to.forward(self.board.active).unwrap());
                self.promote_state = Some(PromoteState { from, to, nonpromote });
            }
            Message::Move(action) => {
                assert!(self.legal_moves.contains(&action));

                if self.playing.is_none_or(|playing| self.board.active == playing) {
                    self.selected = None;
                }

                self.board.play(action);

                self.legal_moves.clear();
                self.board.legal_moves(&mut self.legal_moves);

                self.last_move = Some(action);
                self.move_options = Bitboard::EMPTY;
                self.promote_state = None;

                if let Some(selected) = self.selected {
                    self.update_move_options(selected);
                }

                self.under_check = if self.board.is_check()
                    && let Some(king) =
                        (self.board[PieceKind::King] & self.board[self.board.active]).bitscan()
                {
                    Some(king)
                } else {
                    None
                };
            }
            Message::FlipBoard => self.face_up = !self.face_up,
        }
    }
    pub fn flipped(&self) -> bool {
        self.face_up == Side::Gote
    }
    fn update_move_options(&mut self, selected: SelectedPiece) {
        self.move_options = self
            .legal_moves
            .iter()
            .filter(|&&mov| selected.matches(mov))
            .map(|mov| mov.to())
            .collect();
    }
    fn selected_side(&self, selected: SelectedPiece) -> Side {
        match selected {
            SelectedPiece::Board(sq) => {
                self.board.pieces.side(sq).expect("selected piece should exist")
            }
            SelectedPiece::Hand(piece) => piece.side(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SelectedPiece {
    Board(Square),
    Hand(Piece),
}

#[derive(Debug, Clone)]
struct Bounds {
    left_hand: Rectangle,
    board: Rectangle,
    right_hand: Rectangle,
}

impl Widget<Message, Theme, Renderer> for &BoardState {
    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &Limits,
    ) -> layout::Node {
        let height = limits.max().height.min(limits.max().width * 9.0 / 11.0);
        let width = limits.max().width.min(limits.max().height * 11.0 / 9.0);

        layout::Node::new(Size::new(width, height))
    }
    fn size(&self) -> Size<Length> {
        Size { width: Length::Fill, height: Length::Fill }
    }
    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &iced::advanced::renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        self.draw_(layout, theme, cursor, renderer);
    }
    fn update(
        &mut self,
        _state: &mut iced::advanced::widget::Tree,
        event: &iced::Event,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        if self.selected.is_some() {
            // FIXME: ideally we only redraw the board once, and just redraw the selected piece each frame.
            shell.request_redraw();
        }

        let Some(cursor) = cursor.position() else { return };
        let iced::Event::Mouse(event) = event else { return };

        let bounds = BoardState::bounds(layout);

        let is_pressed = match event {
            mouse::Event::ButtonPressed(mouse::Button::Left) => true,
            mouse::Event::ButtonReleased(mouse::Button::Left) => false,
            mouse::Event::CursorLeft | mouse::Event::ButtonPressed(mouse::Button::Right) => {
                shell.publish(Message::Selected(None));
                return;
            }
            _ => return,
        };
        if let Some(PromoteState { from, to, nonpromote }) = self.promote_state {
            let Some(selected) = self.select_square(&bounds, cursor) else { return };

            if selected != to && selected != nonpromote {
                return;
            }
            let promoted = selected == to;
            shell.publish(Message::Move(Move::Board { from, to, promoted }));
            return;
        }
        if is_pressed {
            shell.publish({
                let piece = self.select_piece(&bounds, cursor);
                Message::Selected(piece)
            });
            return;
        }

        let Some(from) = self.selected else { return };

        if let Some(playing) = self.playing
            && playing != self.selected_side(from)
        {
            shell.publish(Message::Selected(None));
            return;
        }

        let Some(to) = self.select_square(&bounds, cursor) else {
            shell.publish(Message::Selected(None));
            return;
        };

        let message = match from {
            SelectedPiece::Board(from) => {
                let nonpromote = Move::Board { from, to, promoted: false };
                let promote = Move::Board { from, to, promoted: true };

                let has_nonpromote = self.legal_moves.contains(&nonpromote);
                let has_promote = self.legal_moves.contains(&promote);

                match (has_promote, has_nonpromote) {
                    (false, false) => Message::Selected(None),
                    (false, true) => Message::Move(nonpromote),
                    (true, false) => Message::Move(promote),
                    (true, true) => Message::Promote(from, to),
                }
            }
            SelectedPiece::Hand(piece) => {
                if piece.side() == self.board.active
                    && self.legal_moves.contains(&Move::Drop { piece: piece.kind(), to })
                {
                    Message::Move(Move::Drop { piece: piece.kind(), to })
                } else {
                    Message::Selected(None)
                }
            }
        };
        shell.publish(message);
    }
    fn mouse_interaction(
        &self,
        _tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.selected.is_some() {
            return mouse::Interaction::Grabbing;
        }
        let Some(cursor) = cursor.position() else { return mouse::Interaction::None };
        if !layout.bounds().contains(cursor) {
            return mouse::Interaction::None;
        }
        let bounds = BoardState::bounds(layout);
        if bounds.left_hand.contains(cursor) || bounds.right_hand.contains(cursor) {
            return mouse::Interaction::Grab;
        }
        if let Some(PromoteState { to, nonpromote, .. }) = self.promote_state
            && (self.piece_rect(bounds.board, to).contains(cursor)
                || self.piece_rect(bounds.board, nonpromote).contains(cursor))
        {
            return mouse::Interaction::Pointer;
        }
        if let Some(square) = self.select_square(&bounds, cursor)
            && self.board.pieces.contains(square)
        {
            return mouse::Interaction::Grab;
        }
        mouse::Interaction::None
    }
}

impl BoardState {
    fn bounds(layout: Layout<'_>) -> Bounds {
        let bounds = layout.bounds();
        let square_size = bounds.height / 9.0;
        Bounds {
            board: Rectangle {
                x: bounds.center_x() - bounds.height / 2.0,
                y: bounds.center_y() - bounds.height / 2.0,
                width: bounds.height,
                height: bounds.height,
            },
            left_hand: Rectangle {
                x: bounds.x,
                y: bounds.y,
                width: square_size,
                height: bounds.height - square_size * 2.0,
            },
            right_hand: Rectangle {
                x: bounds.x + square_size * 10.0,
                y: bounds.y + square_size * 2.0,
                width: square_size,
                height: bounds.height - square_size * 2.0,
            },
        }
    }

    fn select_piece(&self, bounds: &Bounds, cursor: Point) -> Option<SelectedPiece> {
        if let Some(square) = self.select_square(bounds, cursor) {
            self.board.pieces.get(square)?; // don't bother selecting if the piece doesn't exist
            Some(SelectedPiece::Board(square))
        } else {
            self.select_from_hand(bounds, cursor)
        }
    }
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn select_from_hand(&self, bounds: &Bounds, cursor: Point) -> Option<SelectedPiece> {
        for (side, hand) in [(self.face_up, bounds.right_hand), (!self.face_up, bounds.left_hand)] {
            if !hand.contains(cursor) {
                continue;
            }
            let mut rel_cursor = cursor - hand.position();
            rel_cursor.y /= hand.height / 7.0;
            let kind = PieceKind::from_int(if side == self.face_up {
                (7.0 - rel_cursor.y) as u8
            } else {
                rel_cursor.y as u8
            })?;
            if kind == PieceKind::King {
                return None;
            }
            return Some(SelectedPiece::Hand(Piece::new(side, kind, false)));
        }
        None
    }
    fn piece_rect(&self, bounds: Rectangle, sq: Square) -> Rectangle {
        let sq = if self.flipped() { sq.flip() } else { sq };
        Rectangle {
            x: bounds.x + (bounds.width / 9.0) * sq.file() as u8 as f32,
            y: bounds.y + (bounds.height / 9.0) * sq.rank() as u8 as f32,
            width: bounds.width / 9.0,
            height: bounds.height / 9.0,
        }
    }
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn select_square(&self, bounds: &Bounds, cursor: Point) -> Option<Square> {
        if !bounds.board.contains(cursor) {
            return None;
        }
        let board = bounds.board;
        let mut rel_cursor = cursor - board.position();
        rel_cursor.x /= board.width / 9.0;
        rel_cursor.y /= board.height / 9.0;
        let sq =
            Square::new(File::from_int(rel_cursor.x as u8)?, Rank::from_int(rel_cursor.y as u8)?);
        Some(if self.flipped() { sq.flip() } else { sq })
    }
}

impl<'a, Handle> From<&'a BoardState> for Element<'a, Message, Theme, Renderer>
where
    Renderer: image::Renderer<Handle = Handle>,
    Handle: Clone + 'a,
{
    fn from(state: &'a BoardState) -> Element<'a, Message, Theme, Renderer> {
        Element::new(state)
    }
}

impl SelectedPiece {
    pub fn matches(self, action: Move) -> bool {
        match (action, self) {
            (Move::Board { from, .. }, Self::Board(selected)) => from == selected,
            (Move::Drop { piece, .. }, Self::Hand(selected)) => piece == selected.kind(),
            _ => false,
        }
    }
}
