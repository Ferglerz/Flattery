use super::*;

pub(super) fn create(params: Arc<FlatteryParams>, shared: Arc<Shared>) -> Option<Box<dyn Editor>> {
    create_vizia_editor(
        params.editor_state.clone(),
        ViziaTheming::Custom,
        move |cx, _| {
            nih_plug_vizia::assets::register_noto_sans_light(cx);
            FlatteryView {
                params: params.clone(),
                shared: shared.clone(),
                layout: GraphLayout::default(),
                font: Cell::new(None),
                drag: None,
                hover_low_cut: false,
                hover_high_cut: false,
                hover_max_boost: false,
                hover_max_cut: false,
                hover_strength: None,
                hover_curve: None,
                hover_node: None,
                selected: None,
                mouse: (0.0, 0.0),
                hover: None,
                edit: None,
                graph_zoomed: false,
            }
            .build(cx, |cx| {
                let timer = cx.add_timer(Duration::from_millis(16), None, |cx, action| {
                    if let TimerAction::Tick(_) = action {
                        cx.needs_redraw();
                    }
                });
                cx.start_timer(timer);
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        },
    )
}
