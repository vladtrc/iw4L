use std::path::PathBuf;

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, save_to_disk};

use crate::class_select::ClassSelectOverlayOpen;
use crate::class_setup::{ClassEditRow, ClassSetupScratch};
use crate::menu::MenuEnabled;
use crate::options::{OptionsControlGroup, OptionsDepth, OptionsState, OptionsTab};
use crate::retail_menu::RetailMenuStack;

const MENU_SHOT_SETTLE: u32 = 24;
const MENU_SHOT_WRITE: u32 = 20;

#[derive(Resource, Clone, Debug)]
pub struct MenuShotPlan {
    pub dir: PathBuf,
}

impl MenuShotPlan {
    pub fn from_env() -> Option<Self> {
        let dir = std::env::var_os("IW4L_MENU_SHOT_DIR")?;
        Some(Self {
            dir: PathBuf::from(dir),
        })
    }
}

#[derive(Default)]
pub(crate) struct MenuShotMachine {
    step: usize,
    wait: u32,
    capture_pending: bool,
}

pub(crate) fn run_menu_shots(
    mut commands: Commands,
    mut machine: Local<MenuShotMachine>,
    mut exit: MessageWriter<AppExit>,
    mut stack: ResMut<RetailMenuStack>,
    mut scratch: ResMut<ClassSetupScratch>,
    mut options: ResMut<OptionsState>,
    mut menu_enabled: ResMut<MenuEnabled>,
    mut class_overlay: ResMut<ClassSelectOverlayOpen>,
    mut focus: ResMut<crate::nav::Focus>,
    plan: Res<MenuShotPlan>,
) {
    if machine.wait > 0 {
        machine.wait -= 1;
        return;
    }
    if machine.capture_pending {
        machine.capture_pending = false;
        machine.step += 1;
    }

    let dir = &plan.dir;
    if let Err(error) = std::fs::create_dir_all(dir) {
        diag::warn!(Ui, "menu-shots: cannot create {}: {error}", dir.display());
        exit.write(AppExit::from_code(1));
        return;
    }

    match machine.step {
        0 => {
            scratch.slots[0].primary = "iw4:weapon/ak47_mp".into();
            scratch.slots[0].primary_attachments.clear();
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 1;
        }
        1 => {
            capture_menu(&mut commands, dir, "01-main-menu");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        2 => {
            open(&mut stack, "options");
            options.tab = OptionsTab::Video;
            options.depth = OptionsDepth::SectionRows;
            focus.widget = Some("options/resolution".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 3;
        }
        3 => {
            capture_menu(&mut commands, dir, "02-options-video");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        4 => {
            options.tab = OptionsTab::Audio;
            options.depth = OptionsDepth::SectionRows;
            focus.widget = Some("options/volume".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 5;
        }
        5 => {
            capture_menu(&mut commands, dir, "03-options-audio");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        6 => {
            options.tab = OptionsTab::Controls;
            options.depth = OptionsDepth::SectionRows;
            focus.widget = Some("options/movement".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 7;
        }
        7 => {
            capture_menu(&mut commands, dir, "04-options-controls");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        8 => {
            options.depth = OptionsDepth::ControlBinds;
            options.control_group = OptionsControlGroup::Movement;
            focus.widget = Some("options/binds/27".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 9;
        }
        9 => {
            capture_menu(&mut commands, dir, "05-options-movement");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        10 => {
            options.control_group = OptionsControlGroup::Actions;
            focus.widget = Some("options/binds/1".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 11;
        }
        11 => {
            capture_menu(&mut commands, dir, "06-options-actions");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        12 => {
            options.control_group = OptionsControlGroup::Look;
            focus.widget = Some("options/binds/41".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 13;
        }
        13 => {
            capture_menu(&mut commands, dir, "07-options-look");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        14 => {
            open(&mut stack, "options");
            options.tab = OptionsTab::Multiplayer;
            options.depth = OptionsDepth::SectionRows;
            focus.widget = Some("options/player_name".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 15;
        }
        15 => {
            capture_menu(&mut commands, dir, "08-options-multiplayer");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        16 => {
            options.tab = OptionsTab::Game;
            options.depth = OptionsDepth::SectionRows;
            focus.widget = Some("options/sensitivity".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 17;
        }
        17 => {
            capture_menu(&mut commands, dir, "09-options-game");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        18 => {
            stack.names.truncate(1);
            stack.names.push("class_setup".into());
            scratch.editing = None;
            scratch.editing_attachment = None;
            scratch.rename_buffer = None;
            scratch.selected = 0;
            focus.widget = Some("class_setup/slot/0".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 19;
        }
        19 => {
            capture_menu(&mut commands, dir, "10-class-setup");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        20 => {
            scratch.selected = 0;
            scratch.summary_active = true;
            scratch.editing = None;
            scratch.picker_category = None;
            focus.widget = Some("class_setup/primary".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 21;
        }
        21 => {
            capture_menu(&mut commands, dir, "11-cac-edit-rows");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        22 => {
            scratch.editing = Some(ClassEditRow::Primary);
            focus.widget = Some("class_setup/cat/sniper".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 23;
        }
        23 => {
            capture_menu(&mut commands, dir, "12-cac-categories");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        24 => {
            scratch.picker_category = Some(assets::CacAuthoredCategory::Sniper);
            focus.widget = Some("class_setup/pick/0".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 25;
        }
        25 => {
            capture_menu(&mut commands, dir, "13-cac-snipers");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        26 => {
            scratch.editing = None;
            scratch.picker_category = None;
            scratch.editing_attachment = Some(ClassEditRow::Primary);
            focus.widget = Some("class_setup/attachment_none".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 27;
        }
        27 => {
            capture_menu(&mut commands, dir, "14-cac-attachments");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        28 => {
            scratch.editing_attachment = None;
            scratch.rename_buffer = Some("assault".into());
            focus.widget = Some("class_setup/rename_buffer".into());
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 29;
        }
        29 => {
            capture_menu(&mut commands, dir, "15-cac-rename");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        30 => {
            scratch.rename_buffer = None;
            menu_enabled.0 = true;
            class_overlay.0 = true;
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 31;
        }
        31 => {
            capture_menu(&mut commands, dir, "16-class-select");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        32 => {
            class_overlay.0 = false;
            open(&mut stack, "options");
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 33;
        }
        33 => {
            options.tab = OptionsTab::Video;
            options.depth = OptionsDepth::ResolutionPicker;
            focus.widget = Some(if options.display_resolutions.is_empty() {
                "options/resolution_cancel".into()
            } else {
                "options/resolution_choice/0".into()
            });
            machine.wait = MENU_SHOT_SETTLE;
            machine.step = 34;
        }
        34 => {
            capture_menu(&mut commands, dir, "17-options-resolution-picker");
            machine.wait = MENU_SHOT_WRITE;
            machine.capture_pending = true;
        }
        35 => {
            diag::info!(Ui, "menu-shots: wrote pack under {}", dir.display());
            exit.write(AppExit::Success);
            machine.step = 36;
        }
        _ => {}
    }
}

fn open(stack: &mut RetailMenuStack, name: &str) {
    stack.names.truncate(1);
    stack.names.push(name.to_owned());
}

fn capture_menu(commands: &mut Commands, dir: &std::path::Path, name: &str) {
    let out = dir.join(format!("{name}.png"));
    diag::info!(Ui, "menu-shot write: {}", out.display());
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(out));
}
