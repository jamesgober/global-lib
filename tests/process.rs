//! The process-wide free functions. Tests run in parallel and share the
//! process-wide store, so each test uses names of its own.

#![allow(clippy::unwrap_used, missing_docs)]

use std::sync::Arc;
use std::thread;

use global_lib::Error;

#[test]
fn test_free_functions_round_trip() {
    global_lib::set("t1.port", 7000u16);
    assert!(global_lib::contains("t1.port"));
    assert_eq!(global_lib::value::<u16>("t1.port"), Some(7000));
    assert_eq!(*global_lib::get::<u16>("t1.port").unwrap(), 7000);
    assert_eq!(*global_lib::try_get::<u16>("t1.port").unwrap(), 7000);
    assert_eq!(global_lib::with::<u16, _>("t1.port", |p| p + 1), Some(7001));
    global_lib::edit::<u16>("t1.port", |p| *p = 8000).unwrap();
    assert_eq!(global_lib::value::<u16>("t1.port"), Some(8000));
    assert!(global_lib::remove("t1.port"));
    assert!(!global_lib::contains("t1.port"));
}

#[test]
fn test_free_function_errors() {
    assert!(matches!(
        global_lib::try_get::<u8>("t2.none"),
        Err(Error::Missing { .. })
    ));
    global_lib::set("t2.v", 1u8);
    assert!(matches!(
        global_lib::try_get::<i64>("t2.v"),
        Err(Error::TypeMismatch {
            found: "u8",
            expected: "i64",
            ..
        })
    ));
    assert!(matches!(
        global_lib::edit::<i64>("t2.v", |_| {}),
        Err(Error::TypeMismatch { .. })
    ));
    global_lib::define("t2.const", "fixed").unwrap();
    assert!(matches!(
        global_lib::define("t2.const", "other"),
        Err(Error::AlreadyDefined { .. })
    ));
}

#[test]
fn test_visible_from_every_thread() {
    global_lib::set("t3.shared", String::from("hello"));
    let seen: Vec<_> = (0..8)
        .map(|_| thread::spawn(|| global_lib::value::<String>("t3.shared")))
        .collect::<Vec<_>>()
        .into_iter()
        .map(|h| h.join().unwrap())
        .collect();
    assert!(seen.iter().all(|s| s.as_deref() == Some("hello")));
}

#[test]
fn test_unsized_friendly_types_work() {
    global_lib::set("t4.list", vec![1u32, 2, 3]);
    global_lib::set("t4.shared", Arc::new(5u8));
    global_lib::set(
        "t4.closure",
        Arc::new(|x: u32| x * 2) as Arc<dyn Fn(u32) -> u32 + Send + Sync>,
    );
    assert_eq!(
        global_lib::with::<Vec<u32>, _>("t4.list", Vec::len),
        Some(3)
    );
    let f = global_lib::value::<Arc<dyn Fn(u32) -> u32 + Send + Sync>>("t4.closure").unwrap();
    assert_eq!(f(21), 42);
}
