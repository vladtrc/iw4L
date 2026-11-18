use super::wire::{WireError, WireReader, WireWriter};

pub(crate) trait Wire: Sized {
    fn put(&self, out: &mut WireWriter);
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError>;
}

pub(crate) trait Via<T> {
    fn put(value: &T, out: &mut WireWriter);
    fn get(input: &mut WireReader<'_>) -> Result<T, WireError>;
}

macro_rules! wire_primitive {
    ($($t:ty => $put:ident $get:ident),* $(,)?) => {
        $(impl Wire for $t {
            fn put(&self, out: &mut WireWriter) {
                out.$put(*self);
            }
            fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
                input.$get()
            }
        })*
    };
}

wire_primitive!(
    u8 => put_u8 get_u8,
    i8 => put_i8 get_i8,
    u16 => put_u16 get_u16,
    u32 => put_u32 get_u32,
    u64 => put_u64 get_u64,
    i32 => put_i32 get_i32,
    f32 => put_f32 get_f32,
);

impl Wire for bool {
    fn put(&self, out: &mut WireWriter) {
        out.put_u8(u8::from(*self));
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        match input.get_u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(WireError::Malformed("bad bool")),
        }
    }
}

impl<T: Wire + Default, const N: usize> Wire for [T; N] {
    fn put(&self, out: &mut WireWriter) {
        for value in self {
            value.put(out);
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let mut array: [T; N] = std::array::from_fn(|_| T::default());
        for slot in &mut array {
            *slot = T::get(input)?;
        }
        Ok(array)
    }
}

impl<T: Wire> Wire for Option<T> {
    fn put(&self, out: &mut WireWriter) {
        match self {
            None => out.put_u8(0),
            Some(value) => {
                out.put_u8(1);
                value.put(out);
            }
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        match input.get_u8()? {
            0 => Ok(None),
            1 => T::get(input).map(Some),
            _ => Err(WireError::Malformed("bad option tag")),
        }
    }
}

impl<A: Wire, B: Wire> Wire for (A, B) {
    fn put(&self, out: &mut WireWriter) {
        self.0.put(out);
        self.1.put(out);
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok((A::get(input)?, B::get(input)?))
    }
}

impl<A: Wire, B: Wire, C: Wire> Wire for (A, B, C) {
    fn put(&self, out: &mut WireWriter) {
        self.0.put(out);
        self.1.put(out);
        self.2.put(out);
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        Ok((A::get(input)?, B::get(input)?, C::get(input)?))
    }
}

impl Wire for String {
    fn put(&self, out: &mut WireWriter) {
        debug_assert!(self.len() <= u16::MAX as usize);
        out.put_u16(self.len() as u16);
        out.put_bytes(self.as_bytes());
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let mut bytes = vec![0; usize::from(input.get_u16()?)];
        input.get_bytes(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| WireError::Malformed("text is not UTF-8"))
    }
}

pub(crate) fn put_len<L: TryFrom<usize> + Wire>(len: usize, out: &mut WireWriter) {
    let Ok(len) = L::try_from(len) else {
        panic!("wire length {len} overflows its prefix");
    };
    len.put(out);
}

fn get_items<T: Wire>(count: usize, input: &mut WireReader<'_>) -> Result<Vec<T>, WireError> {
    let mut items = Vec::with_capacity(count.min(256));
    for _ in 0..count {
        items.push(T::get(input)?);
    }
    Ok(items)
}

pub(crate) fn put_slice<T: Wire>(items: &[T], out: &mut WireWriter) {
    put_len::<u16>(items.len(), out);
    for item in items {
        item.put(out);
    }
}

impl<T: Wire> Wire for Vec<T> {
    fn put(&self, out: &mut WireWriter) {
        put_slice(self, out);
    }
    fn get(input: &mut WireReader<'_>) -> Result<Self, WireError> {
        let count = usize::from(input.get_u16()?);
        get_items(count, input)
    }
}

pub(crate) struct Len8;

impl<T: Wire> Via<Vec<T>> for Len8 {
    fn put(value: &Vec<T>, out: &mut WireWriter) {
        put_len::<u8>(value.len(), out);
        for item in value {
            item.put(out);
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Vec<T>, WireError> {
        let count = usize::from(input.get_u8()?);
        get_items(count, input)
    }
}

pub(crate) struct Len32;

impl<T: Wire> Via<Vec<T>> for Len32 {
    fn put(value: &Vec<T>, out: &mut WireWriter) {
        put_len::<u32>(value.len(), out);
        for item in value {
            item.put(out);
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Vec<T>, WireError> {
        let count = input.get_u32()? as usize;
        if count > u16::MAX as usize {
            return Err(WireError::Malformed("u32 length beyond any wire table"));
        }
        get_items(count, input)
    }
}

pub(crate) struct Bytes;

impl<const N: usize> Via<[u8; N]> for Bytes {
    fn put(value: &[u8; N], out: &mut WireWriter) {
        out.put_bytes(value);
    }
    fn get(input: &mut WireReader<'_>) -> Result<[u8; N], WireError> {
        let mut bytes = [0; N];
        input.get_bytes(&mut bytes)?;
        Ok(bytes)
    }
}

pub(crate) struct Text8;

impl Via<String> for Text8 {
    fn put(value: &String, out: &mut WireWriter) {
        put_len::<u8>(value.len(), out);
        out.put_bytes(value.as_bytes());
    }
    fn get(input: &mut WireReader<'_>) -> Result<String, WireError> {
        let mut bytes = vec![0; usize::from(input.get_u8()?)];
        input.get_bytes(&mut bytes)?;
        String::from_utf8(bytes).map_err(|_| WireError::Malformed("text is not UTF-8"))
    }
}

pub(crate) struct Sentinel8;

impl Via<Option<u8>> for Sentinel8 {
    fn put(value: &Option<u8>, out: &mut WireWriter) {
        out.put_u8(value.unwrap_or(u8::MAX));
    }
    fn get(input: &mut WireReader<'_>) -> Result<Option<u8>, WireError> {
        let value = input.get_u8()?;
        Ok((value != u8::MAX).then_some(value))
    }
}

pub(crate) struct Sentinel16;

impl Via<Option<u16>> for Sentinel16 {
    fn put(value: &Option<u16>, out: &mut WireWriter) {
        out.put_u16(value.unwrap_or(u16::MAX));
    }
    fn get(input: &mut WireReader<'_>) -> Result<Option<u16>, WireError> {
        let value = input.get_u16()?;
        Ok((value != u16::MAX).then_some(value))
    }
}

pub(crate) struct Padded;

impl<T: Wire + Default> Via<Option<T>> for Padded {
    fn put(value: &Option<T>, out: &mut WireWriter) {
        value.is_some().put(out);
        match value {
            Some(value) => value.put(out),
            None => T::default().put(out),
        }
    }
    fn get(input: &mut WireReader<'_>) -> Result<Option<T>, WireError> {
        let present = bool::get(input)?;
        let value = T::get(input)?;
        Ok(present.then_some(value))
    }
}

macro_rules! wire_field_put {
    ($value:expr, $out:ident) => {
        $crate::transport::codec::Wire::put($value, $out)
    };
    ($value:expr, $out:ident, $via:ty) => {
        <$via as $crate::transport::codec::Via<_>>::put($value, $out)
    };
}

macro_rules! wire_field_get {
    ($input:ident) => {
        $crate::transport::codec::Wire::get($input)?
    };
    ($input:ident, $via:ty) => {
        <$via as $crate::transport::codec::Via<_>>::get($input)?
    };
}

macro_rules! wire_struct {
    (
        $t:ty { $($field:ident $(: $via:ty)?),* $(,)? }
        $(rest $rest:expr;)?
        $(check $check:expr => $message:literal;)?
    ) => {
        impl $crate::transport::codec::Wire for $t {
            fn put(&self, out: &mut $crate::transport::wire::WireWriter) {
                $($crate::transport::codec::wire_field_put!(&self.$field, out $(, $via)?);)*
            }
            fn get(
                input: &mut $crate::transport::wire::WireReader<'_>,
            ) -> Result<Self, $crate::transport::wire::WireError> {
                let value = Self {
                    $($field: $crate::transport::codec::wire_field_get!(input $(, $via)?),)*
                    $(..$rest)?
                };
                $(if !($check)(&value) {
                    return Err($crate::transport::wire::WireError::Malformed($message));
                })?
                Ok(value)
            }
        }
    };
}

macro_rules! wire_enum {
    (
        $t:ty, $message:literal {
            $($tag:literal => $variant:ident
                $({ $($field:ident $(: $via:ty)?),* $(,)? })?
                $(( $($item:ident $(: $item_via:ty)?),* $(,)? ))?
            ),* $(,)?
        }
    ) => {
        impl $crate::transport::codec::Wire for $t {
            #[allow(unused_variables)]
            fn put(&self, out: &mut $crate::transport::wire::WireWriter) {
                match self {
                    $(Self::$variant $({ $($field),* })? $(( $($item),* ))? => {
                        out.put_u8($tag);
                        $($($crate::transport::codec::wire_field_put!($field, out $(, $via)?);)*)?
                        $($($crate::transport::codec::wire_field_put!($item, out $(, $item_via)?);)*)?
                    })*
                }
            }
            fn get(
                input: &mut $crate::transport::wire::WireReader<'_>,
            ) -> Result<Self, $crate::transport::wire::WireError> {
                Ok(match input.get_u8()? {
                    $($tag => Self::$variant
                        $({ $($field: $crate::transport::codec::wire_field_get!(input $(, $via)?)),* })?
                        $(( $($crate::transport::codec::wire_field_get!(input $(, $item_via)?)),* ))?,)*
                    _ => return Err($crate::transport::wire::WireError::Malformed($message)),
                })
            }
        }
    };
}

macro_rules! wire_newtype {
    ($($t:ty),* $(,)?) => {
        $(impl $crate::transport::codec::Wire for $t {
            fn put(&self, out: &mut $crate::transport::wire::WireWriter) {
                $crate::transport::codec::Wire::put(&self.0, out);
            }
            fn get(
                input: &mut $crate::transport::wire::WireReader<'_>,
            ) -> Result<Self, $crate::transport::wire::WireError> {
                Ok(Self($crate::transport::codec::Wire::get(input)?))
            }
        })*
    };
}

pub(crate) use {wire_enum, wire_field_get, wire_field_put, wire_newtype, wire_struct};
