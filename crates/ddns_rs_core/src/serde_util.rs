use serde::de::{Deserialize, Deserializer, Error, SeqAccess, Visitor};
use serde::ser::{Serialize, SerializeSeq, Serializer};
use std::fmt;
use std::marker::PhantomData;

/// A serde deserializer that treats both missing fields AND explicit `null`
/// as an empty Vec, matching Go's `encoding/json` behavior for slices.
pub fn deserialize_null_default_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct VecVisitor<T>(PhantomData<T>);

    impl<'de, T> Visitor<'de> for VecVisitor<T>
    where
        T: Deserialize<'de>,
    {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a sequence or null")
        }

        fn visit_unit<E: Error>(self) -> Result<Vec<T>, E> {
            Ok(Vec::new())
        }

        fn visit_none<E: Error>(self) -> Result<Vec<T>, E> {
            Ok(Vec::new())
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<T>, A::Error> {
            let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(item) = seq.next_element()? {
                vec.push(item);
            }
            Ok(vec)
        }
    }

    deserializer.deserialize_any(VecVisitor(PhantomData))
}

/// Serialize an empty Vec as `[]` (used to keep round-tripping sane).
pub fn serialize_vec<T: Serialize, S: Serializer>(
    vec: &Vec<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let mut seq = serializer.serialize_seq(Some(vec.len()))?;
    for item in vec {
        seq.serialize_element(item)?;
    }
    seq.end()
}
