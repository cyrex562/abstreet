use crate::layout::Widget;
use crate::{
    Button, Color, DrawBoth, EventCtx, Filler, GeomBatch, GfxCtx, HorizontalAlignment, JustDraw,
    Plot, ScreenDims, ScreenPt, ScreenRectangle, Slider, Text, VerticalAlignment,
};
use geom::{Distance, Duration, Polygon};
use std::collections::{HashMap, HashSet};
use stretch::geometry::{Rect, Size};
use stretch::node::{Node, Stretch};
use stretch::style::{AlignItems, Dimension, FlexDirection, FlexWrap, JustifyContent, Style};

pub struct ManagedWidget {
    widget: WidgetType,
    style: LayoutStyle,
    rect: ScreenRectangle,
    bg: Option<DrawBoth>,
}

enum WidgetType {
    Draw(JustDraw),
    Btn(Button),
    Slider(String),
    Filler(String),
    // TODO Sadness. Can't have some kind of wildcard generic here?
    DurationPlot(Plot<Duration>),
    UsizePlot(Plot<usize>),
    Row(Vec<ManagedWidget>),
    Column(Vec<ManagedWidget>),
}

struct LayoutStyle {
    bg_color: Option<Color>,
    align_items: Option<AlignItems>,
    justify_content: Option<JustifyContent>,
    flex_wrap: Option<FlexWrap>,
    size: Option<Size<Dimension>>,
    padding: Option<Rect<Dimension>>,
    margin: Option<Rect<Dimension>>,
}

impl LayoutStyle {
    fn apply(&self, style: &mut Style) {
        if let Some(x) = self.align_items {
            style.align_items = x;
        }
        if let Some(x) = self.justify_content {
            style.justify_content = x;
        }
        if let Some(x) = self.flex_wrap {
            style.flex_wrap = x;
        }
        if let Some(x) = self.size {
            style.size = x;
        }
        if let Some(x) = self.padding {
            style.padding = x;
        }
        if let Some(x) = self.margin {
            style.margin = x;
        }
    }
}

// Layouting
// TODO Maybe I just want margin, not padding. And maybe more granular controls per side. And to
// apply margin to everything in a row or column.
// TODO Row and columns feel backwards when using them.
impl ManagedWidget {
    pub fn centered(mut self) -> ManagedWidget {
        self.style.align_items = Some(AlignItems::Center);
        self.style.justify_content = Some(JustifyContent::SpaceAround);
        self
    }

    pub fn centered_cross(mut self) -> ManagedWidget {
        self.style.align_items = Some(AlignItems::Center);
        self
    }

    pub fn evenly_spaced(mut self) -> ManagedWidget {
        self.style.justify_content = Some(JustifyContent::SpaceBetween);
        self
    }

    pub fn flex_wrap(mut self, ctx: &EventCtx) -> ManagedWidget {
        self.style.flex_wrap = Some(FlexWrap::Wrap);
        self.style.justify_content = Some(JustifyContent::SpaceAround);
        self.style.size = Some(Size {
            width: Dimension::Points(ctx.canvas.window_width as f32),
            height: Dimension::Undefined,
        });
        self
    }

    pub fn bg(mut self, color: Color) -> ManagedWidget {
        self.style.bg_color = Some(color);
        self
    }

    pub fn padding(mut self, pixels: usize) -> ManagedWidget {
        self.style.padding = Some(Rect {
            start: Dimension::Points(pixels as f32),
            end: Dimension::Points(pixels as f32),
            top: Dimension::Points(pixels as f32),
            bottom: Dimension::Points(pixels as f32),
        });
        self
    }

    pub fn margin(mut self, pixels: usize) -> ManagedWidget {
        self.style.margin = Some(Rect {
            start: Dimension::Points(pixels as f32),
            end: Dimension::Points(pixels as f32),
            top: Dimension::Points(pixels as f32),
            bottom: Dimension::Points(pixels as f32),
        });
        self
    }
}

// Convenient?? constructors
impl ManagedWidget {
    fn new(widget: WidgetType) -> ManagedWidget {
        ManagedWidget {
            widget,
            style: LayoutStyle {
                bg_color: None,
                align_items: None,
                justify_content: None,
                flex_wrap: None,
                size: None,
                padding: None,
                margin: None,
            },
            rect: ScreenRectangle::placeholder(),
            bg: None,
        }
    }

    pub fn draw_batch(ctx: &EventCtx, batch: GeomBatch) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Draw(JustDraw::wrap(DrawBoth::new(
            ctx,
            batch,
            Vec::new(),
        ))))
    }

    pub fn just_draw(j: JustDraw) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Draw(j))
    }

    pub fn draw_text(ctx: &EventCtx, txt: Text) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Draw(JustDraw::text(txt, ctx)))
    }

    pub fn draw_svg(ctx: &EventCtx, filename: &str) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Draw(JustDraw::svg(filename, ctx)))
    }

    pub fn btn(btn: Button) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Btn(btn))
    }

    pub fn slider(label: &str) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Slider(label.to_string()))
    }

    pub fn filler(label: &str) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Filler(label.to_string()))
    }

    pub(crate) fn duration_plot(plot: Plot<Duration>) -> ManagedWidget {
        ManagedWidget::new(WidgetType::DurationPlot(plot))
    }

    pub(crate) fn usize_plot(plot: Plot<usize>) -> ManagedWidget {
        ManagedWidget::new(WidgetType::UsizePlot(plot))
    }

    pub fn row(widgets: Vec<ManagedWidget>) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Row(widgets))
    }

    pub fn col(widgets: Vec<ManagedWidget>) -> ManagedWidget {
        ManagedWidget::new(WidgetType::Column(widgets))
    }
}

// Internals
impl ManagedWidget {
    fn event(
        &mut self,
        ctx: &mut EventCtx,
        sliders: &mut HashMap<String, Slider>,
    ) -> Option<Outcome> {
        match self.widget {
            WidgetType::Draw(_) => {}
            WidgetType::Btn(ref mut btn) => {
                btn.event(ctx);
                if btn.clicked() {
                    return Some(Outcome::Clicked(btn.action.clone()));
                }
            }
            WidgetType::Slider(ref name) => {
                sliders.get_mut(name).unwrap().event(ctx);
            }
            WidgetType::Filler(_) | WidgetType::DurationPlot(_) | WidgetType::UsizePlot(_) => {}
            WidgetType::Row(ref mut widgets) | WidgetType::Column(ref mut widgets) => {
                for w in widgets {
                    if let Some(o) = w.event(ctx, sliders) {
                        return Some(o);
                    }
                }
            }
        }
        None
    }

    fn draw(&self, g: &mut GfxCtx, sliders: &HashMap<String, Slider>) {
        if let Some(ref bg) = self.bg {
            bg.redraw(ScreenPt::new(self.rect.x1, self.rect.y1), g);
        }

        match self.widget {
            WidgetType::Draw(ref j) => j.draw(g),
            WidgetType::Btn(ref btn) => btn.draw(g),
            WidgetType::Slider(ref name) => sliders[name].draw(g),
            WidgetType::Filler(_) => {}
            WidgetType::DurationPlot(ref plot) => plot.draw(g),
            WidgetType::UsizePlot(ref plot) => plot.draw(g),
            WidgetType::Row(ref widgets) | WidgetType::Column(ref widgets) => {
                for w in widgets {
                    w.draw(g, sliders);
                }
            }
        }
    }

    // Populate a flattened list of Nodes, matching the traversal order
    fn get_flexbox(
        &self,
        parent: Node,
        sliders: &HashMap<String, Slider>,
        fillers: &HashMap<String, Filler>,
        stretch: &mut Stretch,
        nodes: &mut Vec<Node>,
    ) {
        // TODO Can I use | in the match and "cast" to Widget?
        let widget: &dyn Widget = match self.widget {
            WidgetType::Draw(ref widget) => widget,
            WidgetType::Btn(ref widget) => widget,
            WidgetType::Slider(ref name) => &sliders[name],
            WidgetType::Filler(ref name) => &fillers[name],
            WidgetType::DurationPlot(ref widget) => widget,
            WidgetType::UsizePlot(ref widget) => widget,
            WidgetType::Row(ref widgets) => {
                let mut style = Style {
                    flex_direction: FlexDirection::Row,
                    ..Default::default()
                };
                self.style.apply(&mut style);
                let row = stretch.new_node(style, Vec::new()).unwrap();
                nodes.push(row);
                for widget in widgets {
                    widget.get_flexbox(row, sliders, fillers, stretch, nodes);
                }
                stretch.add_child(parent, row).unwrap();
                return;
            }
            WidgetType::Column(ref widgets) => {
                let mut style = Style {
                    flex_direction: FlexDirection::Column,
                    ..Default::default()
                };
                self.style.apply(&mut style);
                let col = stretch.new_node(style, Vec::new()).unwrap();
                nodes.push(col);
                for widget in widgets {
                    widget.get_flexbox(col, sliders, fillers, stretch, nodes);
                }
                stretch.add_child(parent, col).unwrap();
                return;
            }
        };
        let dims = widget.get_dims();
        let mut style = Style {
            size: Size {
                width: Dimension::Points(dims.width as f32),
                height: Dimension::Points(dims.height as f32),
            },
            ..Default::default()
        };
        self.style.apply(&mut style);
        let node = stretch.new_node(style, Vec::new()).unwrap();
        stretch.add_child(parent, node).unwrap();
        nodes.push(node);
    }

    fn apply_flexbox(
        &mut self,
        sliders: &mut HashMap<String, Slider>,
        fillers: &mut HashMap<String, Filler>,
        stretch: &Stretch,
        nodes: &mut Vec<Node>,
        dx: f64,
        dy: f64,
        scroll_y_offset: f64,
        ctx: &EventCtx,
    ) {
        let result = stretch.layout(nodes.pop().unwrap()).unwrap();
        let x: f64 = result.location.x.into();
        let y: f64 = result.location.y.into();
        let width: f64 = result.size.width.into();
        let height: f64 = result.size.height.into();
        let top_left = match self.widget {
            WidgetType::Slider(ref name) => {
                if name == "scrollbar" {
                    ScreenPt::new(x + dx, y + dy)
                } else {
                    ScreenPt::new(x + dx, y + dy - scroll_y_offset)
                }
            }
            _ => ScreenPt::new(x + dx, y + dy - scroll_y_offset),
        };
        self.rect = ScreenRectangle::top_left(top_left, ScreenDims::new(width, height));
        if let Some(color) = self.style.bg_color {
            // Assume widgets don't dynamically change, so we just upload the background once.
            if self.bg.is_none() {
                let batch = GeomBatch::from(vec![(
                    color,
                    Polygon::rounded_rectangle(
                        Distance::meters(width),
                        Distance::meters(height),
                        Distance::meters(5.0),
                    ),
                )]);
                self.bg = Some(DrawBoth::new(ctx, batch, Vec::new()));
            }
        }

        match self.widget {
            WidgetType::Draw(ref mut widget) => {
                widget.set_pos(top_left);
            }
            WidgetType::Btn(ref mut widget) => {
                widget.set_pos(top_left);
            }
            WidgetType::Slider(ref name) => {
                sliders.get_mut(name).unwrap().set_pos(top_left);
            }
            WidgetType::Filler(ref name) => {
                fillers.get_mut(name).unwrap().set_pos(top_left);
            }
            WidgetType::DurationPlot(ref mut widget) => {
                widget.set_pos(top_left);
            }
            WidgetType::UsizePlot(ref mut widget) => {
                widget.set_pos(top_left);
            }
            WidgetType::Row(ref mut widgets) => {
                // layout() doesn't return absolute position; it's relative to the container.
                for widget in widgets {
                    widget.apply_flexbox(
                        sliders,
                        fillers,
                        stretch,
                        nodes,
                        x + dx,
                        y + dy,
                        scroll_y_offset,
                        ctx,
                    );
                }
            }
            WidgetType::Column(ref mut widgets) => {
                for widget in widgets {
                    widget.apply_flexbox(
                        sliders,
                        fillers,
                        stretch,
                        nodes,
                        x + dx,
                        y + dy,
                        scroll_y_offset,
                        ctx,
                    );
                }
            }
        }
    }

    fn get_all_click_actions(&self, actions: &mut HashSet<String>) {
        match self.widget {
            WidgetType::Draw(_)
            | WidgetType::Slider(_)
            | WidgetType::Filler(_)
            | WidgetType::DurationPlot(_)
            | WidgetType::UsizePlot(_) => {}
            WidgetType::Btn(ref btn) => {
                if actions.contains(&btn.action) {
                    panic!(
                        "Two buttons in one Composite both use action {}",
                        btn.action
                    );
                }
                actions.insert(btn.action.clone());
            }
            WidgetType::Row(ref widgets) | WidgetType::Column(ref widgets) => {
                for w in widgets {
                    w.get_all_click_actions(actions);
                }
            }
        }
    }
}

pub struct Composite {
    top_level: ManagedWidget,
    pos: CompositePosition,

    sliders: HashMap<String, Slider>,
    fillers: HashMap<String, Filler>,

    // TODO This doesn't clip. There's no way to express that the scrollable thing should occupy a
    // small part of the screen.
    // TODO Horizontal scrolling?
    scrollable: bool,
}

pub enum Outcome {
    Clicked(String),
}

enum CompositePosition {
    FillScreen,
    MinimalTopLeft(ScreenPt),
    Aligned(HorizontalAlignment, VerticalAlignment),
}

const SCROLL_SPEED: f64 = 5.0;

impl Composite {
    fn new(top_level: ManagedWidget, pos: CompositePosition) -> Composite {
        Composite {
            top_level,
            pos,

            sliders: HashMap::new(),
            fillers: HashMap::new(),

            scrollable: false,
        }
    }

    pub fn minimal_size(ctx: &EventCtx, top_level: ManagedWidget, top_left: ScreenPt) -> Composite {
        let mut c = Composite::new(top_level, CompositePosition::MinimalTopLeft(top_left));
        c.recompute_layout(ctx);
        c
    }

    pub fn minimal_size_with_fillers(
        ctx: &EventCtx,
        top_level: ManagedWidget,
        top_left: ScreenPt,
        fillers: Vec<(&str, Filler)>,
    ) -> Composite {
        let mut c = Composite::new(top_level, CompositePosition::MinimalTopLeft(top_left));
        for (name, filler) in fillers {
            c.fillers.insert(name.to_string(), filler);
        }
        c.recompute_layout(ctx);
        c
    }

    pub fn fill_screen(ctx: &EventCtx, top_level: ManagedWidget) -> Composite {
        let mut c = Composite::new(top_level, CompositePosition::FillScreen);
        c.recompute_layout(ctx);
        c
    }

    pub fn aligned(
        ctx: &EventCtx,
        (horiz, vert): (HorizontalAlignment, VerticalAlignment),
        top_level: ManagedWidget,
    ) -> Composite {
        let mut c = Composite::new(top_level, CompositePosition::Aligned(horiz, vert));
        c.recompute_layout(ctx);
        c
    }

    pub fn aligned_with_sliders(
        ctx: &EventCtx,
        (horiz, vert): (HorizontalAlignment, VerticalAlignment),
        top_level: ManagedWidget,
        sliders: Vec<(&str, Slider)>,
    ) -> Composite {
        let mut c = Composite::new(top_level, CompositePosition::Aligned(horiz, vert));
        for (name, slider) in sliders {
            c.sliders.insert(name.to_string(), slider);
        }
        c.recompute_layout(ctx);
        c
    }

    pub fn scrollable(ctx: &EventCtx, top_level: ManagedWidget) -> Composite {
        let mut c = Composite::new(
            top_level,
            CompositePosition::Aligned(HorizontalAlignment::Left, VerticalAlignment::Top),
        );
        c.scrollable = true;
        // TODO Height of our panel
        c.sliders.insert(
            "scrollbar".to_string(),
            Slider::vertical(ctx, ctx.canvas.window_height - 100.0),
        );
        c.top_level = ManagedWidget::row(vec![c.top_level, ManagedWidget::slider("scrollbar")]);
        c.recompute_layout(ctx);
        c
    }

    fn recompute_layout(&mut self, ctx: &EventCtx) {
        let mut stretch = Stretch::new();
        let root = stretch
            .new_node(
                match self.pos {
                    CompositePosition::FillScreen => Style {
                        size: Size {
                            width: Dimension::Points(ctx.canvas.window_width as f32),
                            height: Dimension::Points(ctx.canvas.window_height as f32),
                        },
                        ..Default::default()
                    },
                    CompositePosition::MinimalTopLeft(_) | CompositePosition::Aligned(_, _) => {
                        Style {
                            // TODO There a way to encode the offset in stretch?
                            ..Default::default()
                        }
                    }
                },
                Vec::new(),
            )
            .unwrap();

        let mut nodes = vec![];
        self.top_level
            .get_flexbox(root, &self.sliders, &self.fillers, &mut stretch, &mut nodes);
        nodes.reverse();

        stretch.compute_layout(root, Size::undefined()).unwrap();
        let top_left = match self.pos {
            CompositePosition::FillScreen => ScreenPt::new(0.0, 0.0),
            CompositePosition::MinimalTopLeft(pt) => pt,
            CompositePosition::Aligned(horiz, vert) => {
                let result = stretch.layout(root).unwrap();
                ctx.canvas.align_window(
                    ScreenDims::new(result.size.width.into(), result.size.height.into()),
                    horiz,
                    vert,
                )
            }
        };
        let offset = self.scroll_y_offset(ctx);
        self.top_level.apply_flexbox(
            &mut self.sliders,
            &mut self.fillers,
            &stretch,
            &mut nodes,
            top_left.x,
            top_left.y,
            offset,
            ctx,
        );
        assert!(nodes.is_empty());
    }

    fn scroll_y_offset(&self, ctx: &EventCtx) -> f64 {
        if self.scrollable {
            self.slider("scrollbar").get_percent()
                * (self.top_level.rect.height() - ctx.canvas.window_height).max(0.0)
        } else {
            0.0
        }
    }

    pub fn set_scroll_y_offset(&mut self, ctx: &EventCtx, offset: f64) {
        assert!(self.scrollable);
        let max = (self.top_level.rect.height() - ctx.canvas.window_height).max(0.0);
        if max == 0.0 {
            assert_eq!(offset, 0.0);
            self.mut_slider("scrollbar").set_percent(ctx, 0.0);
        } else {
            self.mut_slider("scrollbar").set_percent(ctx, offset / max);
        }
        self.recompute_layout(ctx);
    }

    pub fn event(&mut self, ctx: &mut EventCtx) -> Option<Outcome> {
        if self.scrollable
            && self
                .top_level
                .rect
                .contains(ctx.canvas.get_cursor_in_screen_space())
        {
            if let Some(scroll) = ctx.input.get_mouse_scroll() {
                let offset = self.scroll_y_offset(ctx) - scroll * SCROLL_SPEED;
                let max = (self.top_level.rect.height() - ctx.canvas.window_height).max(0.0);
                // TODO Do the clamping in there instead
                self.set_scroll_y_offset(ctx, abstutil::clamp(offset, 0.0, max));
            }
        }

        if ctx.input.is_window_resized() {
            self.recompute_layout(ctx);
        }

        let before = self.scroll_y_offset(ctx);
        let result = self.top_level.event(ctx, &mut self.sliders);
        if self.scroll_y_offset(ctx) != before {
            self.recompute_layout(ctx);
        }
        result
    }

    pub fn draw(&self, g: &mut GfxCtx) {
        g.canvas.mark_covered_area(self.top_level.rect.clone());
        self.top_level.draw(g, &self.sliders);
    }

    pub fn get_all_click_actions(&self) -> HashSet<String> {
        let mut actions = HashSet::new();
        self.top_level.get_all_click_actions(&mut actions);
        actions
    }

    pub fn preserve_scroll(&self, ctx: &EventCtx) -> f64 {
        assert!(self.scrollable);
        self.scroll_y_offset(ctx)
    }

    pub fn restore_scroll(&mut self, ctx: &EventCtx, offset: f64) {
        assert!(self.scrollable);
        self.set_scroll_y_offset(ctx, offset);
    }

    pub fn slider(&self, name: &str) -> &Slider {
        &self.sliders[name]
    }
    pub fn mut_slider(&mut self, name: &str) -> &mut Slider {
        self.sliders.get_mut(name).unwrap()
    }
    pub fn take_slider(&mut self, name: &str) -> Slider {
        self.sliders.remove(name).unwrap()
    }

    pub fn filler_rect(&self, name: &str) -> ScreenRectangle {
        let f = &self.fillers[name];
        ScreenRectangle::top_left(f.top_left, f.dims)
    }
}
