use playerstate_iw4::PlayerState;

pub fn add_predictable_event(ps: &mut PlayerState, event: i32, parm: i32) {
    if event == 0 {
        return;
    }
    match ps.event_sequence & 3 {
        0 => {
            ps.events_0 = event;
            ps.event_parms_0 = parm;
        }
        1 => {
            ps.events_1 = event;
            ps.event_parms_1 = parm;
        }
        2 => {
            ps.events_2 = event;
            ps.event_parms_2 = parm;
        }
        _ => {
            ps.events_3 = event;
            ps.event_parms_3 = parm;
        }
    }
    ps.event_sequence = ps.event_sequence.wrapping_add(1) & 0x7ff;
}

pub fn pm_add_event(ps: &mut PlayerState, event: i32) {
    add_predictable_event(ps, event, 0);
}

const EVENT_SEQUENCE_MASK: i32 = 0x7ff;
const EVENT_SEQUENCE_WRAP_WINDOW: i32 = 0x200;
const EVENT_RING_LEN: i32 = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SequencedPlayerEvent {
    pub sequence: i32,
    pub event: i32,
    pub event_parm: i32,
}

fn ring_slot(ps: &PlayerState, slot: usize) -> (i32, i32) {
    match slot {
        0 => (ps.events_0, ps.event_parms_0),
        1 => (ps.events_1, ps.event_parms_1),
        2 => (ps.events_2, ps.event_parms_2),
        _ => (ps.events_3, ps.event_parms_3),
    }
}

pub fn consume_player_events(
    ps: &PlayerState,
    consumed_sequence: &mut i32,
    mut dispatch: impl FnMut(SequencedPlayerEvent),
) {
    let next = ps.event_sequence;
    if *consumed_sequence == next {
        return;
    }
    if next == 0 {
        *consumed_sequence = 0;
        return;
    }

    let mut consumed = *consumed_sequence;
    if next + EVENT_SEQUENCE_WRAP_WINDOW < consumed {
        consumed -= EVENT_SEQUENCE_MASK + 1;
    }
    if next - consumed > EVENT_RING_LEN {
        consumed = next - EVENT_RING_LEN;
    }

    while consumed < next {
        let slot = (consumed as u32 & 3) as usize;
        let (event, event_parm) = ring_slot(ps, slot);
        if event != 0 {
            dispatch(SequencedPlayerEvent {
                sequence: consumed & EVENT_SEQUENCE_MASK,
                event,
                event_parm,
            });
        }
        consumed += 1;
    }
    *consumed_sequence = next;
}
