/// Logs an event at an explicit level.
///
/// ```ignore
/// event!(Level::Info, "listening on {addr}");
/// event!(target: "audit", Level::Warn, user = %name, attempts = 3, "locked out");
/// ```
///
/// Fields come first, each `name = value` (numbers, booleans and strings),
/// `name = %value` (`Display`) or `name = ?value` (`Debug`); the optional
/// message is a format string and its arguments.
#[macro_export]
macro_rules! event {
    (target: $target:expr, $level:expr, $($rest:tt)+) => {
        $crate::__event!(@parse ($target) ($level) [] $($rest)+)
    };
    ($level:expr, $($rest:tt)+) => {
        $crate::__event!(@parse (::core::module_path!()) ($level) [] $($rest)+)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __value {
    (value, $v:expr) => {
        $crate::field::IntoFieldValue::field_value(&$v)
    };
    (display, $v:expr) => {
        $crate::field::display(&$v)
    };
    (debug, $v:expr) => {
        $crate::field::debug(&$v)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __event {
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident = % $v:expr, $($rest:tt)*) => {
        $crate::__event!(@parse ($t) ($l) [$($f)* (::core::stringify!($name), display, $v)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident = ? $v:expr, $($rest:tt)*) => {
        $crate::__event!(@parse ($t) ($l) [$($f)* (::core::stringify!($name), debug, $v)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident = $v:expr, $($rest:tt)*) => {
        $crate::__event!(@parse ($t) ($l) [$($f)* (::core::stringify!($name), value, $v)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident = % $v:expr) => {
        $crate::__event!(@emit ($t) ($l) [$($f)* (::core::stringify!($name), display, $v)] (::std::string::String::new()))
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident = ? $v:expr) => {
        $crate::__event!(@emit ($t) ($l) [$($f)* (::core::stringify!($name), debug, $v)] (::std::string::String::new()))
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident = $v:expr) => {
        $crate::__event!(@emit ($t) ($l) [$($f)* (::core::stringify!($name), value, $v)] (::std::string::String::new()))
    };
    // a bare `name` records the variable of that name
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident, $($rest:tt)*) => {
        $crate::__event!(@parse ($t) ($l) [$($f)* (::core::stringify!($name), value, $name)] $($rest)*)
    };
    // `%name` and `?name` record the variable of that name.
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] % $name:ident, $($rest:tt)*) => {
        $crate::__event!(@parse ($t) ($l) [$($f)* (::core::stringify!($name), display, $name)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] ? $name:ident, $($rest:tt)*) => {
        $crate::__event!(@parse ($t) ($l) [$($f)* (::core::stringify!($name), debug, $name)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] % $name:ident) => {
        $crate::__event!(@emit ($t) ($l) [$($f)* (::core::stringify!($name), display, $name)] (::std::string::String::new()))
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] ? $name:ident) => {
        $crate::__event!(@emit ($t) ($l) [$($f)* (::core::stringify!($name), debug, $name)] (::std::string::String::new()))
    };
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $name:ident) => {
        $crate::__event!(@emit ($t) ($l) [$($f)* (::core::stringify!($name), value, $name)] (::std::string::String::new()))
    };
    // fields only
    (@parse ($t:expr) ($l:expr) [$($f:tt)*]) => {
        $crate::__event!(@emit ($t) ($l) [$($f)*] (::std::string::String::new()))
    };
    // whatever remains is the message
    (@parse ($t:expr) ($l:expr) [$($f:tt)*] $($message:tt)+) => {
        $crate::__event!(@emit ($t) ($l) [$($f)*] (::std::format!($($message)+)))
    };
    (@emit ($t:expr) ($l:expr) [$(($name:expr, $kind:ident, $v:expr))*] ($message:expr)) => {{
        let __level = $l;
        let __target = $t;
        if $crate::enabled(__level, __target) {
            $crate::dispatch_event(__level, __target, $message, ::std::vec![$(($name, $crate::__value!($kind, $v))),*]);
        }
    }};
}

/// `event!` at `Level::Trace`.
#[macro_export]
macro_rules! trace {
    ($($arg:tt)+) => { $crate::event!($crate::Level::Trace, $($arg)+) };
}

/// `event!` at `Level::Debug`.
#[macro_export]
macro_rules! debug {
    ($($arg:tt)+) => { $crate::event!($crate::Level::Debug, $($arg)+) };
}

/// `event!` at `Level::Info`.
#[macro_export]
macro_rules! info {
    ($($arg:tt)+) => { $crate::event!($crate::Level::Info, $($arg)+) };
}

/// `event!` at `Level::Warn`.
#[macro_export]
macro_rules! warn {
    ($($arg:tt)+) => { $crate::event!($crate::Level::Warn, $($arg)+) };
}

/// `event!` at `Level::Error`.
#[macro_export]
macro_rules! error {
    ($($arg:tt)+) => { $crate::event!($crate::Level::Error, $($arg)+) };
}

/// Creates a span: `span!(Level::Info, "request", id = 7, path = %path)`.
/// Fields may be declared empty with `quench_log::field::Empty` and recorded
/// later through [`Span::record`](crate::Span::record).
#[macro_export]
macro_rules! span {
    (target: $target:expr, $level:expr, $name:literal $(, $($rest:tt)*)?) => {
        $crate::__span!(@parse ($target) ($level) ($name) [] $($($rest)*)?)
    };
    ($level:expr, $name:literal $(, $($rest:tt)*)?) => {
        $crate::__span!(@parse (::core::module_path!()) ($level) ($name) [] $($($rest)*)?)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __span {
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*] $name:ident = % $v:expr, $($rest:tt)*) => {
        $crate::__span!(@parse ($t) ($l) ($n) [$($f)* (::core::stringify!($name), display, $v)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*] $name:ident = ? $v:expr, $($rest:tt)*) => {
        $crate::__span!(@parse ($t) ($l) ($n) [$($f)* (::core::stringify!($name), debug, $v)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*] $name:ident = $v:expr, $($rest:tt)*) => {
        $crate::__span!(@parse ($t) ($l) ($n) [$($f)* (::core::stringify!($name), value, $v)] $($rest)*)
    };
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*] $name:ident = % $v:expr) => {
        $crate::__span!(@make ($t) ($l) ($n) [$($f)* (::core::stringify!($name), display, $v)])
    };
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*] $name:ident = ? $v:expr) => {
        $crate::__span!(@make ($t) ($l) ($n) [$($f)* (::core::stringify!($name), debug, $v)])
    };
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*] $name:ident = $v:expr) => {
        $crate::__span!(@make ($t) ($l) ($n) [$($f)* (::core::stringify!($name), value, $v)])
    };
    (@parse ($t:expr) ($l:expr) ($n:literal) [$($f:tt)*]) => {
        $crate::__span!(@make ($t) ($l) ($n) [$($f)*])
    };
    (@make ($t:expr) ($l:expr) ($n:literal) [$(($name:expr, $kind:ident, $v:expr))*]) => {{
        let __level = $l;
        let __target = $t;
        if $crate::enabled(__level, __target) {
            $crate::Span::new(__level, __target, $n, ::std::vec![$(($name, $crate::__value!($kind, $v))),*])
        } else {
            $crate::Span::none()
        }
    }};
}

/// `span!` at `Level::Trace`.
#[macro_export]
macro_rules! trace_span {
    ($($arg:tt)+) => { $crate::span!($crate::Level::Trace, $($arg)+) };
}

/// `span!` at `Level::Debug`.
#[macro_export]
macro_rules! debug_span {
    ($($arg:tt)+) => { $crate::span!($crate::Level::Debug, $($arg)+) };
}

/// `span!` at `Level::Info`.
#[macro_export]
macro_rules! info_span {
    ($($arg:tt)+) => { $crate::span!($crate::Level::Info, $($arg)+) };
}

/// `span!` at `Level::Warn`.
#[macro_export]
macro_rules! warn_span {
    ($($arg:tt)+) => { $crate::span!($crate::Level::Warn, $($arg)+) };
}

/// `span!` at `Level::Error`.
#[macro_export]
macro_rules! error_span {
    ($($arg:tt)+) => { $crate::span!($crate::Level::Error, $($arg)+) };
}
