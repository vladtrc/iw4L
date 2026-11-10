macro_rules! scalar_get {
    (bool, $dvars:ident, $($field:ident).+) => {
        u8::from($dvars.$($field).+) as f32
    };
    (float, $dvars:ident, $($field:ident).+) => {
        $dvars.$($field).+
    };
}
pub(crate) use scalar_get;

macro_rules! scalar_set {
    (bool, $dvars:ident, $($field:ident).+, $value:expr) => {
        $dvars.$($field).+ = $value != 0.0
    };
    (float, $dvars:ident, $($field:ident).+, $value:expr) => {
        $dvars.$($field).+ = $value
    };
}
pub(crate) use scalar_set;

macro_rules! scalar_is_bool {
    (bool) => {
        true
    };
    (float) => {
        false
    };
}
pub(crate) use scalar_is_bool;

macro_rules! debug_scalars {
    (
        $ty:ident;
        $(
            $name:literal {
                path: [$($field:ident).+],
                min: $min:literal,
                max: $max:literal,
                kind: $kind:ident
            }
        ),+ $(,)?
    ) => {
        const NAMES: &[&str] = &[$($name,)+];

        fn scalar_current(dvars: &$ty, setting: &str) -> (f32, f32, f32, bool) {
            match setting {
                $(
                    $name => (
                        $crate::debug_scalar::scalar_get!($kind, dvars, $($field).+),
                        $min,
                        $max,
                        $crate::debug_scalar::scalar_is_bool!($kind),
                    ),
                )+
                _ => unreachable!(),
            }
        }

        fn scalar_assign(dvars: &mut $ty, setting: &str, value: f32) {
            match setting {
                $(
                    $name => {
                        $crate::debug_scalar::scalar_set!($kind, dvars, $($field).+, value);
                    }
                )+
                _ => unreachable!(),
            }
        }

        pub(crate) fn route(
            mut commands: bevy::prelude::MessageReader<$crate::ConsoleCommand>,
            mut dvars: bevy::prelude::ResMut<$ty>,
            mut console: bevy::prelude::ResMut<$crate::ConsoleState>,
            settings: bevy::prelude::Res<$crate::ConsoleSettings>,
            mut line: bevy::prelude::ResMut<$crate::ConsoleLine>,
        ) {
            for cmd in commands.read() {
                let name = cmd.name.as_str();
                if !NAMES.contains(&name) {
                    continue;
                }
                let parsed = match cmd.args.as_slice() {
                    [] => Ok(None),
                    [value] => value
                        .parse::<f32>()
                        .ok()
                        .filter(|v| v.is_finite())
                        .map(Some)
                        .ok_or(()),
                    _ => Err(()),
                };
                let (current, min, max, is_bool) = scalar_current(&dvars, name);
                let msg = match parsed {
                    Ok(None) => format!("{name} = {current} (domain {min}..{max})"),
                    Ok(Some(value))
                        if (min..=max).contains(&value)
                            && (!is_bool || value == 0.0 || value == 1.0) =>
                    {
                        scalar_assign(&mut dvars, name, value);
                        format!("{name} = {value}")
                    }
                    _ => format!("usage: {name} [finite value {min}..{max}]"),
                };
                diag::info!(Console, "{msg}");
                line.0 = msg.clone();
                console.echo(msg, settings.log_capacity);
            }
        }
    };
}
pub(crate) use debug_scalars;
