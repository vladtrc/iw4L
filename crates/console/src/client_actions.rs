use bevy::prelude::*;
use net::{ActionEnqueueError, ClientActionInbox, LookState, look_angles_from_degrees};
use sim::{ClientAction, ClientId};

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ClientActionTarget<'w> {
    inbox: Option<ResMut<'w, ClientActionInbox>>,
    look: ResMut<'w, LookState>,
}

pub(crate) struct ClientActionQueue<'a, 'w> {
    inbox: &'a mut ClientActionInbox,
    // Borrow the wrapper so only an accepted Move marks LookState changed.
    look: &'a mut ResMut<'w, LookState>,
}

impl<'w> ClientActionTarget<'w> {
    pub(crate) fn queue(&mut self) -> Option<ClientActionQueue<'_, 'w>> {
        Some(ClientActionQueue {
            inbox: self.inbox.as_deref_mut()?,
            look: &mut self.look,
        })
    }
}

impl ClientActionQueue<'_, '_> {
    pub(crate) fn push(
        &mut self,
        client: ClientId,
        action: ClientAction,
    ) -> Result<(), ActionEnqueueError> {
        let angles = match &action {
            ClientAction::Move { angles, .. } => Some(*angles),
            _ => None,
        };
        self.inbox.push(client, action)?;
        if let Some(angles) = angles {
            self.look.angles = look_angles_from_degrees(angles);
        }
        Ok(())
    }
}
