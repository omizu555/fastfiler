// Splitter — ペイン境界をドラッグして幅調整

use floem::event::EventListener;
use floem::prelude::*;
use floem::style::CursorStyle;
use floem::views::{container, label, Decorators};

use crate::state::{AppState, SplitterTarget};
use crate::theme;
pub fn splitter(app: AppState, target: SplitterTarget) -> impl IntoView {
    let drag = app.splitter_drag;
    container(label(|| String::from("")))
        .style(|s| {
            s.width(5.0)
                .height_full()
                .background(theme::border_default())
                .cursor(CursorStyle::ColResize)
        })
        .on_event_stop(EventListener::PointerDown, move |_| {
            drag.set(Some(target));
        })
}
