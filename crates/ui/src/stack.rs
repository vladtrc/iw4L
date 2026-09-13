use crate::UiLayer;
use crate::model::{Modality, Screen, ScreenCmd};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Capture,
    PassThrough,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiSurface {
    pub id: String,
    pub layer: UiLayer,
    pub modality: Modality,
    pub input: InputMode,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UiStack {
    pub surfaces: Vec<UiSurface>,
}

impl UiStack {
    pub fn from_screens(screens: &[Screen]) -> Self {
        Self {
            surfaces: screens.iter().map(surface_of).collect(),
        }
    }

    #[allow(dead_code)]
    pub fn ids(&self) -> Vec<String> {
        self.surfaces.iter().map(|s| s.id.clone()).collect()
    }

    pub fn top_capture_index(&self) -> Option<usize> {
        self.surfaces
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.input == InputMode::Capture)
            .map(|(i, _)| i)
    }

    pub fn paint_start_index(&self) -> usize {
        self.surfaces
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.modality == Modality::Opaque)
            .map(|(i, _)| i)
            .unwrap_or(0)
    }
}

pub fn surface_of(screen: &Screen) -> UiSurface {
    UiSurface {
        id: screen.id.clone(),
        layer: screen.layer,
        modality: screen.modality,
        input: input_of(screen.modality),
    }
}

pub fn input_of(modality: Modality) -> InputMode {
    match modality {
        Modality::Passive => InputMode::PassThrough,
        Modality::Opaque | Modality::Overlay => InputMode::Capture,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackAction {
    Pop,
    RunOnBack,
    OpenQuitConfirm,
}

pub fn back_action(names: &[String], top_on_back: &[ScreenCmd]) -> BackAction {
    if names.len() > 1 {
        BackAction::Pop
    } else if !top_on_back.is_empty() {
        BackAction::RunOnBack
    } else {
        BackAction::OpenQuitConfirm
    }
}
