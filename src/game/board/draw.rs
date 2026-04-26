use std::sync::OnceLock;

use iced::{
    Border,
    advanced::{Text, svg, text::Renderer as _},
    alignment,
    border::Radius,
    widget::text::{Alignment, LineHeight, Shaping, Wrapping},
};

use super::*;

const EMPTY_HAND_OPACITY: f32 = 0.2;

impl BoardState {
    pub fn draw_(&self, layout: Layout, theme: &Theme, cursor: Cursor, renderer: &mut Renderer) {
        let bounds = Self::bounds(layout);
        self.draw_board(bounds.board, theme, renderer);
        self.draw_hands(&bounds, renderer, theme);
        self.draw_selected_piece(layout.bounds(), renderer, cursor);
    }

    pub fn draw_selected_piece(&self, bounds: Rectangle, renderer: &mut Renderer, cursor: Cursor) {
        let Some(cursor) = cursor.position() else { return };
        let Some(selected) = self.selected else { return };

        let mut opacity = 0.9;
        let piece = match selected {
            SelectedPiece::Board(selected) => match self.board.pieces.get(selected) {
                Some(selected) => selected,
                None => return,
            },
            SelectedPiece::Hand(piece) => {
                if self.board.hands[piece.side()][piece.kind()] == 0 {
                    opacity = EMPTY_HAND_OPACITY;
                }
                piece
            }
        };
        let square_size = bounds.height / 9.0;
        let rect = Rectangle {
            x: cursor.x - square_size / 2.0,
            y: cursor.y - square_size / 2.0,
            width: square_size,
            height: square_size,
        };
        let rotation = if piece.side() == self.face_up { Radians(0.0) } else { Radians::PI };
        let image = piece_svg(piece).rotation(rotation).opacity(opacity);
        // infinite bounds so that the piece can be dragged outside the board widget (likely temporary)
        renderer.with_layer(Rectangle::INFINITE, |renderer| {
            renderer.draw_svg(image, rect, rect);
        });
    }
    fn draw_square_color(
        &self,
        sq: Square,
        color: Color,
        bounds: &Rectangle,
        renderer: &mut Renderer,
    ) {
        let sq = if self.flipped() { sq.flip() } else { sq };
        let sq_size = bounds.height / 9.0;
        let rect = Rectangle {
            x: bounds.x + sq_size * sq.file() as u8 as f32,
            y: bounds.y + sq_size * sq.rank() as u8 as f32,
            width: sq_size,
            height: sq_size,
        };
        renderer.fill_quad(Quad { bounds: rect, ..Quad::default() }, Background::Color(color));
    }
    pub fn draw_board(&self, bounds: Rectangle, theme: &Theme, renderer: &mut Renderer) {
        self.draw_board_image(bounds, renderer);
        self.draw_last_move_squares(bounds, renderer);
        self.draw_move_options(bounds, renderer);
        self.draw_check_highligh(bounds, renderer);
        Self::draw_grid_lines(bounds, renderer);
        self.draw_pieces(bounds, renderer);
        self.draw_promote_options(bounds, theme, renderer);
    }

    fn draw_board_image(&self, bounds: Rectangle, renderer: &mut Renderer) {
        renderer.with_layer(bounds, |renderer| {
            renderer.draw_image(self.board_image.clone(), bounds, bounds);
        });
    }

    fn draw_check_highligh(&self, bounds: Rectangle, renderer: &mut Renderer) {
        renderer.with_layer(bounds, |renderer| {
            let Some(pos) = self.under_check else { return };
            self.draw_square_color(pos, CHECK_COLOR, &bounds, renderer);
        });
    }

    fn draw_pieces(&self, bounds: Rectangle, renderer: &mut Renderer) {
        renderer.with_layer(bounds, |renderer| {
            for sq in Square::ALL {
                let Some(piece) = self.board.pieces.get(sq) else { continue };

                let rotation =
                    if piece.side() == self.face_up { Radians(0.0) } else { Radians::PI };
                let opacity =
                    if self.selected == Some(SelectedPiece::Board(sq)) { 0.5 } else { 1.0 };
                let rect = self.piece_rect(bounds, sq);
                let image = piece_svg(piece).opacity(opacity).rotation(rotation);
                renderer.draw_svg(image, rect, rect);
            }
        });
    }

    fn draw_promote_options(&self, bounds: Rectangle, theme: &Theme, renderer: &mut Renderer) {
        let Some(PromoteState { from, to, nonpromote }) = self.promote_state else { return };
        renderer.with_layer(bounds, |renderer| {
            let rotation =
                if self.board.active == self.face_up { Radians(0.0) } else { Radians::PI };

            let promote_rect = self.piece_rect(bounds, to);
            let nonpromote_rect = self.piece_rect(bounds, nonpromote);

            renderer.fill_quad(
                Quad {
                    bounds: promote_rect.union(&nonpromote_rect),
                    border: Border {
                        color: theme.palette().text,
                        width: 0.0,
                        radius: Radius::new(16.0),
                    },
                    ..Quad::default()
                },
                Background::Color(theme.palette().background),
            );

            let kind = self.board.pieces.kind(from).expect("Must be a piece to promote");
            let svg = piece_svg(Piece::new(self.board.active, kind, true));
            renderer.draw_svg(svg.rotation(rotation), promote_rect, promote_rect);

            let svg = piece_svg(Piece::new(self.board.active, kind, false));
            renderer.draw_svg(svg.rotation(rotation), nonpromote_rect, nonpromote_rect);
        });
    }

    fn draw_last_move_squares(&self, bounds: Rectangle, renderer: &mut Renderer) {
        let Some(mov) = self.last_move else { return };
        renderer.with_layer(bounds, |renderer| {
            self.draw_square_color(mov.to(), LAST_MOVE_COLOR, &bounds, renderer);
            if let Action::Move { from, .. } = mov {
                self.draw_square_color(from, LAST_MOVE_COLOR, &bounds, renderer);
            }
        });
    }

    fn draw_move_options(&self, bounds: Rectangle, renderer: &mut Renderer) {
        if self.move_options.is_empty() {
            return;
        }
        renderer.with_layer(bounds, |renderer| {
            for sq in self.move_options {
                self.draw_square_color(sq, MOVE_OPTION_COLOR, &bounds, renderer);
            }
        });
    }
    pub(super) fn draw_hands(&self, bounds: &Bounds, renderer: &mut Renderer, theme: &Theme) {
        for (bounds, side) in [(bounds.left_hand, !self.face_up), (bounds.right_hand, self.face_up)]
        {
            renderer.with_layer(bounds, |renderer| {
                for &piece in &PieceKind::ALL[..PieceKind::King as usize] {
                    let count = self.board.hands[side][piece];
                    let image = piece_svg(Piece::new(side, piece, false));

                    let y = if side == self.face_up {
                        bounds.height - (piece as u8 + 1) as f32 * bounds.width
                    } else {
                        piece as u8 as f32 * bounds.width
                    } + bounds.y;

                    let piece_bounds =
                        Rectangle { x: bounds.x, y, width: bounds.width, height: bounds.width };
                    let opacity = if count == 0 { EMPTY_HAND_OPACITY } else { 1.0 };
                    let rotation = if side == self.face_up { Radians(0.0) } else { Radians::PI };
                    renderer.draw_svg(
                        image.rotation(rotation).opacity(opacity),
                        piece_bounds,
                        piece_bounds,
                    );

                    if count == 0 {
                        continue;
                    }

                    let text_rect = Rectangle {
                        x: piece_bounds.x + piece_bounds.width - 18.0,
                        y: piece_bounds.y + piece_bounds.height - 24.0,
                        width: 18.0,
                        height: 24.0,
                    };

                    renderer.with_layer(piece_bounds, |renderer| {
                        renderer.fill_quad(
                            Quad {
                                bounds: text_rect,
                                border: Border {
                                    color: theme.palette().text,
                                    width: 1.0,
                                    radius: Radius::default(),
                                },
                                ..Quad::default()
                            },
                            Background::Color(theme.palette().background),
                        );
                    });

                    renderer.with_layer(piece_bounds, |renderer| {
                        let text = Text {
                            content: count.to_string(),
                            bounds: text_rect.size(),
                            size: 18.into(),
                            line_height: LineHeight::Absolute(24.into()),
                            font: iced::Font::DEFAULT,
                            align_x: Alignment::Center,
                            align_y: alignment::Vertical::Center,
                            shaping: Shaping::Basic,
                            wrapping: Wrapping::None,
                        };

                        renderer.fill_text(
                            text,
                            text_rect.center(),
                            theme.palette().text,
                            piece_bounds,
                        );
                    });
                }
            });
        }
    }
    fn draw_grid_lines(bounds: Rectangle, renderer: &mut Renderer) {
        renderer.with_layer(bounds, |renderer| {
            let mut draw_line = |offset: Vector, size: Size| {
                let quad = Quad {
                    bounds: Rectangle::new(bounds.position() + offset, size),
                    ..Quad::default()
                };
                renderer.fill_quad(quad, Background::Color(Color::BLACK));
            };
            let line_width = 2.0;
            for i in 0..10u8 {
                let offset = Vector::new((bounds.width - line_width) / 9.0 * i as f32, 0.0);
                let size = Size::new(line_width, bounds.height);
                draw_line(offset, size);
            }
            for i in 0..10u8 {
                let offset = Vector::new(0.0, (bounds.height - line_width) / 9.0 * i as f32);
                let size = Size::new(bounds.width, line_width);
                draw_line(offset, size);
            }
        });
    }
}

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
