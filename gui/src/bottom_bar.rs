use std::collections::VecDeque;

use crate::icon::{self, icon_button, icon_text};
use common::{
    yakui::{
        colored_box, pad, widgets,
        widgets::{List, Pad},
        Color, CrossAxisAlignment, MainAxisAlignment, MainAxisSize,
    },
    BarState, GUICommand, GUIState, BUILDING_TYPE_FACTORY, BUILDING_TYPE_FORGE,
    BUILDING_TYPE_HOUSE,
};

use crate::CONTAINER_BACKGROUND;

pub fn bottom_bar(gui_state: &mut GUIState) {
    let GUIState {
        command_queue,
        bars: bar_state,
        ..
    } = gui_state;
    let mut list = List::row();
    list.main_axis_alignment = MainAxisAlignment::Center;
    list.cross_axis_alignment = CrossAxisAlignment::End;

    list.show(|| {
        let container = widgets::ColoredBox::container(CONTAINER_BACKGROUND);
        container.show_children(|| {
            pad(Pad::balanced(20., 10.), || {
                let mut column = List::column();
                column.main_axis_size = MainAxisSize::Min;
                column.main_axis_alignment = MainAxisAlignment::End;
                column.cross_axis_alignment = CrossAxisAlignment::Center;
                column.item_spacing = 10.;
                column.show(|| {
                    bars(bar_state);
                    build_icons(command_queue);
                });
            });
        });
    });
}

fn bars(bar_state: &BarState) {
    let mut column = List::column();
    column.main_axis_alignment = MainAxisAlignment::End;
    column.cross_axis_alignment = CrossAxisAlignment::Start;
    column.show(|| {
        bar(icon::HEART, Color::RED, bar_state.health_percentage);
        bar(icon::BOLT, Color::BLUE, bar_state.energy_percentage);
    });
}

fn bar(label: &'static str, colour: Color, percentage: f32) {
    let mut row = List::row();
    row.main_axis_size = MainAxisSize::Max;
    row.main_axis_alignment = MainAxisAlignment::Start;
    row.item_spacing = 10.;
    row.cross_axis_alignment = CrossAxisAlignment::Center;
    row.show(|| {
        icon_text(20., label);
        colored_box(colour, [100. * percentage, 10.]);
    });
}

fn build_icons(commands: &mut VecDeque<GUICommand>) {
    let mut row = List::row();
    row.main_axis_size = MainAxisSize::Max;
    row.main_axis_alignment = MainAxisAlignment::Center;
    row.cross_axis_alignment = CrossAxisAlignment::Center;
    row.item_spacing = 10.;

    let mut icon_clicked = None;
    row.show(|| {
        if icon_button(icon::FORGE).clicked {
            icon_clicked = Some(BUILDING_TYPE_FORGE);
        }
        if icon_button(icon::FACTORY).clicked {
            icon_clicked = Some(BUILDING_TYPE_FACTORY);
        }
        if icon_button(icon::HOUSE).clicked {
            icon_clicked = Some(BUILDING_TYPE_HOUSE);
        }
    });

    if let Some(building_type) = icon_clicked {
        commands.push_back(GUICommand::ConstructBuilding(building_type));
    }
}
