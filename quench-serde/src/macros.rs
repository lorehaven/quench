/// Builds a [`Value`](crate::Value) from JSON-looking syntax; interpolated
/// expressions can be anything [`Serialize`](crate::Serialize).
///
/// ```
/// use quench_serde::json;
///
/// let user = "ada";
/// let value = json!({
///     "name": user,
///     "tags": ["a", "b"],
///     "active": true,
///     "limit": null,
///     "n": 1 + 2,
/// });
/// assert_eq!(value["tags"][1], json!("b"));
/// ```
#[macro_export(local_inner_macros)]
macro_rules! json {
    ($($json:tt)+) => {
        json_internal!($($json)+)
    };
}

#[doc(hidden)]
#[macro_export(local_inner_macros)]
macro_rules! json_internal {
    // Arrays: munch `[$($elems,)*]` one element at a time.
    (@array [$($elems:expr,)*]) => {
        ::std::vec![$($elems,)*]
    };
    (@array [$($elems:expr),*]) => {
        ::std::vec![$($elems),*]
    };
    (@array [$($elems:expr,)*] null $($rest:tt)*) => {
        json_internal!(@array [$($elems,)* json_internal!(null)] $($rest)*)
    };
    (@array [$($elems:expr,)*] true $($rest:tt)*) => {
        json_internal!(@array [$($elems,)* json_internal!(true)] $($rest)*)
    };
    (@array [$($elems:expr,)*] false $($rest:tt)*) => {
        json_internal!(@array [$($elems,)* json_internal!(false)] $($rest)*)
    };
    (@array [$($elems:expr,)*] [$($array:tt)*] $($rest:tt)*) => {
        json_internal!(@array [$($elems,)* json_internal!([$($array)*])] $($rest)*)
    };
    (@array [$($elems:expr,)*] {$($map:tt)*} $($rest:tt)*) => {
        json_internal!(@array [$($elems,)* json_internal!({$($map)*})] $($rest)*)
    };
    (@array [$($elems:expr,)*] $next:expr, $($rest:tt)*) => {
        json_internal!(@array [$($elems,)* json_internal!($next),] $($rest)*)
    };
    (@array [$($elems:expr,)*] $last:expr) => {
        json_internal!(@array [$($elems,)* json_internal!($last)])
    };
    (@array [$($elems:expr),*] , $($rest:tt)*) => {
        json_internal!(@array [$($elems,)*] $($rest)*)
    };
    (@array [$($elems:expr),*] $unexpected:tt $($rest:tt)*) => {
        json_unexpected!($unexpected)
    };

    // Objects: `(key tokens so far) (rest) (copy of rest, for error spans)`.
    (@object $object:ident () () ()) => {};
    (@object $object:ident [$($key:tt)+] ($value:expr) , $($rest:tt)*) => {
        let _ = $object.insert(($($key)+), $value);
        json_internal!(@object $object () ($($rest)*) ($($rest)*));
    };
    (@object $object:ident [$($key:tt)+] ($value:expr) $unexpected:tt $($rest:tt)*) => {
        json_unexpected!($unexpected);
    };
    (@object $object:ident [$($key:tt)+] ($value:expr)) => {
        let _ = $object.insert(($($key)+), $value);
    };
    (@object $object:ident ($($key:tt)+) (: null $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!(null)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: true $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!(true)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: false $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!(false)) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: [$($array:tt)*] $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!([$($array)*])) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: {$($map:tt)*} $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!({$($map)*})) $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: $value:expr , $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!($value)) , $($rest)*);
    };
    (@object $object:ident ($($key:tt)+) (: $value:expr) $copy:tt) => {
        json_internal!(@object $object [$($key)+] (json_internal!($value)));
    };
    (@object $object:ident ($($key:tt)+) (:) $copy:tt) => {
        json_internal!();
    };
    (@object $object:ident ($($key:tt)+) () $copy:tt) => {
        json_internal!();
    };
    (@object $object:ident () (: $($rest:tt)*) ($colon:tt $($copy:tt)*)) => {
        json_unexpected!($colon);
    };
    (@object $object:ident ($($key:tt)*) (, $($rest:tt)*) ($comma:tt $($copy:tt)*)) => {
        json_unexpected!($comma);
    };
    (@object $object:ident () (($key:expr) : $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object ($key) (: $($rest)*) (: $($rest)*));
    };
    (@object $object:ident ($($key:tt)*) (: $($unexpected:tt)+) $copy:tt) => {
        json_expect_expr_comma!($($unexpected)+);
    };
    (@object $object:ident ($($key:tt)*) ($tt:tt $($rest:tt)*) $copy:tt) => {
        json_internal!(@object $object ($($key)* $tt) ($($rest)*) ($($rest)*));
    };

    // Entry points.
    (null) => {
        $crate::Value::Null
    };
    (true) => {
        $crate::Value::Bool(true)
    };
    (false) => {
        $crate::Value::Bool(false)
    };
    ([]) => {
        $crate::Value::Array(::std::vec![])
    };
    ([ $($tt:tt)+ ]) => {
        $crate::Value::Array(json_internal!(@array [] $($tt)+))
    };
    ({}) => {
        $crate::Value::Object($crate::Map::new())
    };
    ({ $($tt:tt)+ }) => {
        $crate::Value::Object({
            let mut object = $crate::Map::new();
            json_internal!(@object object () ($($tt)+) ($($tt)+));
            object
        })
    };
    ($other:expr) => {
        $crate::to_value(&$other)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! json_unexpected {
    () => {};
}

#[doc(hidden)]
#[macro_export]
macro_rules! json_expect_expr_comma {
    ($e:expr , $($tt:tt)*) => {};
}

#[cfg(test)]
mod tests {
    use crate::{Map, Value};

    #[test]
    fn literals() {
        assert_eq!(json!(null), Value::Null);
        assert_eq!(json!(true), Value::Bool(true));
        assert_eq!(json!(false), Value::Bool(false));
        assert_eq!(json!(5), Value::from(5));
        assert_eq!(json!(-5), Value::from(-5));
        assert_eq!(json!(1.5), Value::Float(1.5));
        assert_eq!(json!("s"), Value::from("s"));
        assert_eq!(json!([]), Value::Array(vec![]));
        assert_eq!(json!({}), Value::Object(Map::new()));
    }

    #[test]
    fn arrays() {
        assert_eq!(
            json!([1, "two", null, true, false, [3], {"k": 4}]).to_string(),
            r#"[1,"two",null,true,false,[3],{"k":4}]"#
        );
        assert_eq!(json!([1, 2,]).to_string(), "[1,2]");
        assert_eq!(json!([[], {}]).to_string(), "[[],{}]");
    }

    #[test]
    fn objects_keep_the_written_order() {
        let v = json!({
            "z": 1,
            "a": {"nested": [true, null]},
            "m": "x",
            "list": [1, 2, 3],
            "t": true,
            "f": false,
            "n": null,
        });
        assert_eq!(
            v.to_string(),
            r#"{"z":1,"a":{"nested":[true,null]},"m":"x","list":[1,2,3],"t":true,"f":false,"n":null}"#
        );
        assert_eq!(v["a"]["nested"][0], Value::Bool(true));
    }

    #[test]
    fn interpolates_expressions_of_any_serialize_type() {
        let name = String::from("ada");
        let tags = vec!["a", "b"];
        let count = 2_usize;
        let maybe: Option<u8> = None;
        let v = json!({
            "name": name,
            "tags": tags,
            "count": count + 1,
            "maybe": maybe,
            "nested": { "again": name.len() },
            "pair": (1, "x"),
        });
        assert_eq!(
            v.to_string(),
            r#"{"name":"ada","tags":["a","b"],"count":3,"maybe":null,"nested":{"again":3},"pair":[1,"x"]}"#
        );
        assert_eq!(json!([name, count]).to_string(), r#"["ada",2]"#);
        assert_eq!(json!(name).to_string(), r#""ada""#);
    }

    #[test]
    fn computed_keys_go_in_parentheses() {
        let key = format!("k{}", 1);
        let v = json!({ (key.clone()): 1, (format!("{key}b")): 2, "plain": 3 });
        assert_eq!(v.to_string(), r#"{"k1":1,"k1b":2,"plain":3}"#);
    }

    #[test]
    fn unfinished_trailing_values_work() {
        assert_eq!(json!({"a": 1}).to_string(), r#"{"a":1}"#);
        assert_eq!(
            json!({"a": [1], "b": {"c": null}}).to_string(),
            r#"{"a":[1],"b":{"c":null}}"#
        );
    }
}
