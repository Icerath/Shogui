use iced::{
    Length, Point, Renderer, Size, Theme,
    advanced::{Widget, layout, mouse, widget::Tree},
};

use crate::{Element, Message};

/// A Column where each child has equal width
pub struct RColumn<'a> {
    children: Vec<Element<'a>>,
    spacing: f32,
}

impl<'a> RColumn<'a> {
    pub(crate) fn new(children: impl IntoIterator<Item = Element<'a>>) -> Self {
        Self { children: children.into_iter().collect(), spacing: 0.0 }
    }
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self
    }
}

impl Widget<Message, Theme, Renderer> for RColumn<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let mut max_width = f32::MAX;
        let mut remaining_height = limits.max().height;
        let children_heights = (self.children.iter_mut())
            .zip(&mut tree.children)
            .rev()
            .map(|(child, tree)| {
                let layout = child.as_widget_mut().layout(
                    tree,
                    renderer,
                    &limits.max_height(remaining_height),
                );
                remaining_height -= layout.size().height + self.spacing;
                max_width = max_width.min(layout.size().width);
                (layout.size().height, remaining_height)
            })
            .collect::<Vec<_>>();

        let children = (children_heights.iter().rev())
            .zip(self.children.iter_mut())
            .zip(&mut tree.children)
            .map(|(((height, remaining_height), child), tree)| {
                let mut layout = child.as_widget_mut().layout(
                    tree,
                    renderer,
                    &limits.max_height(*height).max_width(max_width),
                );
                layout.move_to_mut(Point::new(
                    layout.bounds().x + (limits.max().width - max_width) * 0.5,
                    layout.bounds().y + (remaining_height),
                ));
                layout
            })
            .collect();
        layout::Node::with_children(limits.max(), children)
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &iced::advanced::renderer::Style,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
    ) {
        for ((layout, element), tree) in layout.children().zip(&self.children).zip(&tree.children) {
            element.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &iced::Event,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn iced::advanced::Clipboard,
        shell: &mut iced::advanced::Shell<'_, Message>,
        viewport: &iced::Rectangle,
    ) {
        for ((layout, element), tree) in
            layout.children().zip(&mut self.children).zip(&mut tree.children)
        {
            element
                .as_widget_mut()
                .update(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: layout::Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &iced::Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        for ((layout, element), tree) in layout.children().zip(&self.children).zip(&tree.children) {
            let interaction =
                element.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer);
            if interaction != mouse::Interaction::None {
                return interaction;
            }
        }
        mouse::Interaction::None
    }
    fn children(&self) -> Vec<Tree> {
        self.children.iter().map(Tree::new).collect()
    }
    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: layout::Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        operation.traverse(&mut |operation| {
            for ((child, layout), tree) in
                self.children.iter_mut().zip(layout.children()).zip(&mut tree.children)
            {
                child.as_widget_mut().operate(tree, layout, renderer, operation);
            }
        });
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.children);
    }
}

impl<'a> From<RColumn<'a>> for Element<'a> {
    fn from(column: RColumn<'a>) -> Self {
        Element::new(column)
    }
}
