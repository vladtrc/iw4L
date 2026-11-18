use crate::PlayerState;

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
