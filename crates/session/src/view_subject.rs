use bevy::prelude::*;
use frame::{LifeFrontPublished, PresentedPublished, ViewSubject};
use net::{ClientSet, LocalPresentClient, PresentedSnapshot};

pub fn publish_view_subject(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    mut subject: ResMut<ViewSubject>,
) {
    let ps = presented.player(local.0);
    *subject = match ps {
        None => ViewSubject::None,
        Some(p) if !p.is_live_frame() => {
            let focus = u32::try_from(p.kill_cam_client_num).ok();
            ViewSubject::Seat {
                viewer: local.0.0,
                focus,
            }
        }
        Some(_) => ViewSubject::Own { client: local.0.0 },
    };
}

pub fn register_view_subject(app: &mut App) {
    app.init_resource::<ViewSubject>()
        .add_systems(Update, publish_view_subject.in_set(PresentedPublished))
        .configure_sets(
            Update,
            LifeFrontPublished
                .in_set(ClientSet::Present)
                .after(publish_view_subject),
        );
}
