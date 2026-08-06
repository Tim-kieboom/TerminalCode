use crate::vec_map::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct TestIndex(usize);
impl VecMapIndex for TestIndex {
    fn new_index(value: usize) -> Self {
        TestIndex(value)
    }

    fn index(&self) -> usize {
        self.0
    }
}

type TestMap<T> = VecMap<TestIndex, T>;

#[test]
fn test_new_and_default() {
    let map: TestMap<i32> = TestMap::new();
    assert_eq!(map.cap(), 0);
    assert_eq!(map.len(), 0);

    let map2: TestMap<i32> = VecMap::default();
    assert_eq!(map, map2);
}

#[test]
fn test_const_default() {
    const _MAP: TestMap<i32> = TestMap::const_default();
}

#[test]
fn test_with_capacity() {
    let map: TestMap<i32> = TestMap::with_capacity(5);
    assert_eq!(map.cap(), 5);
    assert_eq!(map.len(), 0);
    for i in 0..5 {
        assert!(!map.contains(TestIndex(i)));
    }
}

#[test]
fn test_insert_basic() {
    let mut map: TestMap<i32> = TestMap::new();
    assert_eq!(map.insert(TestIndex(0), 42), None);
    assert_eq!(map.cap(), 1);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(TestIndex(0)), Some(&42));
}

#[test]
fn test_insert_grows_vector() {
    let mut map: TestMap<i32> = TestMap::new();
    assert_eq!(map.insert(TestIndex(10), 100), None);
    assert_eq!(map.cap(), 11);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(TestIndex(10)), Some(&100));
    assert_eq!(map.get(TestIndex(0)), None);
}

#[test]
fn test_insert_overwrite() {
    let mut map: TestMap<i32> = TestMap::new();
    assert_eq!(map.insert(TestIndex(0), 42), None);
    assert_eq!(map.insert(TestIndex(0), 99), Some(42));
    assert_eq!(map.get(TestIndex(0)), Some(&99));
}

#[test]
fn test_insert_high_index_after_low() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(0), 5);
    assert_eq!(map.contains(TestIndex(100)), false);
    assert_eq!(map.get(TestIndex(100)), None);
    assert_eq!(map.cap(), 1);
}

#[test]
fn test_get_and_contains() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(1), 10);
    map.insert(TestIndex(3), 30);

    assert!(!map.contains(TestIndex(0)));
    assert!(map.contains(TestIndex(1)));
    assert!(!map.contains(TestIndex(2)));
    assert!(map.contains(TestIndex(3)));

    assert_eq!(map.get(TestIndex(0)), None);
    assert_eq!(map.get(TestIndex(1)), Some(&10));
    assert_eq!(map.get(TestIndex(2)), None);
    assert_eq!(map.get(TestIndex(3)), Some(&30));
}

#[test]
fn test_remove() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(0), 42);
    map.insert(TestIndex(2), 24);

    assert_eq!(map.remove(TestIndex(1)), None);
    assert_eq!(map.len(), 2);

    assert_eq!(map.remove(TestIndex(0)), Some(42));
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(TestIndex(0)), None);

    assert_eq!(map.remove(TestIndex(2)), Some(24));
    assert_eq!(map.len(), 0);
}

#[test]
fn test_remove_and_reinsert() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(0), 5);
    assert_eq!(map.remove(TestIndex(0)), Some(5));
    assert_eq!(map.insert(TestIndex(0), 9), None);
    assert_eq!(map.get(TestIndex(0)), Some(&9));
}

#[test]
fn test_clear() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(0), 1);
    map.insert(TestIndex(2), 2);
    assert_eq!(map.cap(), 3);
    assert_eq!(map.len(), 2);

    map.clear();
    assert_eq!(map.cap(), 0);
    assert_eq!(map.len(), 0);

    map.insert(TestIndex(0), 9);
    assert_eq!(map.cap(), 1);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(TestIndex(0)), Some(&9));
}

#[test]
fn test_from_slice() {
    let slice = vec![(TestIndex(5), "hello"), (TestIndex(0), "world")];
    let map = TestMap::from_slice(&slice);

    assert_eq!(map.cap(), 6);
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(TestIndex(0)), Some(&"world"));
    assert_eq!(map.get(TestIndex(5)), Some(&"hello"));
}

#[test]
fn test_from_slice_empty() {
    let slice: Vec<(TestIndex, &str)> = vec![];
    let map = TestMap::from_slice(&slice);
    assert_eq!(map.cap(), 0);
    assert_eq!(map.len(), 0);
}

#[test]
fn test_from_slice_overlapping() {
    let slice = vec![(TestIndex(2), "a"), (TestIndex(2), "b")];
    let map = TestMap::from_slice(&slice);
    assert_eq!(map.cap(), 3);
    assert_eq!(map.len(), 1);
    assert_eq!(map.get(TestIndex(2)), Some(&"b"));
}

#[test]
fn test_from_vec() {
    let pairs = vec![(TestIndex(2), 200), (TestIndex(7), 700)];
    let map = TestMap::from_vec(pairs);

    assert_eq!(map.cap(), 8);
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(TestIndex(2)), Some(&200));
    assert_eq!(map.get(TestIndex(7)), Some(&700));
}

#[test]
fn test_from_vec_non_monotonic() {
    let pairs = vec![(TestIndex(4), 40), (TestIndex(1), 10), (TestIndex(7), 70)];
    let map = TestMap::from_vec(pairs);
    assert_eq!(map.cap(), 8);
    assert_eq!(map.len(), 3);
    assert_eq!(map.get(TestIndex(1)), Some(&10));
    assert_eq!(map.get(TestIndex(4)), Some(&40));
    assert_eq!(map.get(TestIndex(7)), Some(&70));
}

#[test]
fn test_from_vec_empty() {
    let pairs: Vec<(TestIndex, i32)> = vec![];
    let map = TestMap::from_vec(pairs);
    assert_eq!(map.cap(), 0);
    assert_eq!(map.len(), 0);
}

#[test]
fn test_extend() {
    let mut map: TestMap<i32> = TestMap::new();
    let extra = vec![(TestIndex(1), 10), (TestIndex(3), 30)];
    map.extend(extra.into_iter());

    assert_eq!(map.len(), 2);
    assert_eq!(map.get(TestIndex(1)), Some(&10));
    assert_eq!(map.get(TestIndex(3)), Some(&30));
}

#[test]
fn test_extend_empty() {
    let mut map: TestMap<i32> = TestMap::new();
    map.extend(vec![].into_iter());
    assert_eq!(map.len(), 0);
    assert_eq!(map.cap(), 0);
}

#[test]
fn test_keys_iterator() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(0), 1);
    map.insert(TestIndex(2), 3);
    map.insert(TestIndex(2), 4);

    let keys: Vec<_> = map.keys().collect();
    assert_eq!(keys.len(), 2);
    assert!(keys.contains(&TestIndex(0)));
    assert!(keys.contains(&TestIndex(2)));
}

#[test]
fn test_entries_iterator() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(1), 10);
    map.insert(TestIndex(3), 30);

    let entries: Vec<_> = map.entries().collect();
    assert_eq!(entries.len(), 2);

    let mut found_10 = false;
    let mut found_30 = false;
    for (idx, val) in entries {
        if idx.index() == 1 && *val == 10 {
            found_10 = true;
        }
        if idx.index() == 3 && *val == 30 {
            found_30 = true;
        }
    }
    assert!(found_10);
    assert!(found_30);
}

#[test]
fn test_into_entries() {
    let mut map: TestMap<i32> = TestMap::new();

    let expected = vec![(TestIndex(0), 42), (TestIndex(5), 24)];
    for (index, value) in expected.iter().copied() {
        map.insert(index, value);
    }

    let entries: Vec<_> = map.into_entries().collect();
    assert_eq!(expected, entries);
}

#[test]
fn test_into_entries_empty() {
    let map: TestMap<i32> = TestMap::new();
    let entries: Vec<_> = map.into_entries().collect();
    assert_eq!(entries.len(), 0);
}

#[test]
fn test_values_iterator() {
    let mut map: TestMap<String> = TestMap::new();
    map.insert(TestIndex(2), "foo".to_string());
    map.insert(TestIndex(4), "bar".to_string());

    let values: Vec<_> = map.values().collect();
    assert_eq!(values.len(), 2);
    assert!(values.iter().any(|s| *s == "foo"));
    assert!(values.iter().any(|s| *s == "bar"));
}

#[test]
fn test_into_values() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(1), 10);
    map.insert(TestIndex(3), 30);

    let values: Vec<i32> = map.into_values().collect();
    assert_eq!(values.len(), 2);
    assert!(values.contains(&10));
    assert!(values.contains(&30));
}

#[test]
fn test_get_mut() {
    let mut map: TestMap<i32> = TestMap::new();
    map.insert(TestIndex(0), 42);

    if let Some(val) = map.get_mut(TestIndex(0)) {
        *val = 99;
    }
    assert_eq!(map.get(TestIndex(0)), Some(&99));
}

#[test]
fn test_clone() {
    let mut original: TestMap<String> = TestMap::new();
    original.insert(TestIndex(10), "test".to_string());

    let cloned = original.clone();
    assert_eq!(original.cap(), cloned.cap());
    assert_eq!(original.len(), cloned.len());
    assert_eq!(original.get(TestIndex(10)), cloned.get(TestIndex(10)));
}

#[test]
fn test_partial_eq() {
    let mut map1: TestMap<i32> = TestMap::new();
    map1.insert(TestIndex(0), 1);
    map1.insert(TestIndex(2), 3);

    let mut map2: TestMap<i32> = TestMap::new();
    map2.insert(TestIndex(0), 1);
    map2.insert(TestIndex(2), 3);

    assert_eq!(map1, map2);
}

#[test]
fn test_partial_eq_different_capacities() {
    let mut map1: TestMap<i32> = TestMap::with_capacity(10);
    map1.insert(TestIndex(0), 1);

    let mut map2: TestMap<i32> = TestMap::new();
    map2.insert(TestIndex(0), 1);

    assert_eq!(map1, map2);
}

#[test]
fn test_from_iterator() {
    let iter = vec![(TestIndex(1), 10), (TestIndex(3), 30)];
    let map: TestMap<i32> = iter.into_iter().collect();
    assert_eq!(map.len(), 2);
    assert_eq!(map.get(TestIndex(1)), Some(&10));
    assert_eq!(map.get(TestIndex(3)), Some(&30));
}

#[test]
fn test_serde() {
    let map = TestMap::<i32>::from_vec(vec![
        (TestIndex(0), 1),
        (TestIndex(1), 2),
        (TestIndex(10), 3),
    ]);
    let json = serde_json::to_value(&map).unwrap();
    let new_map: TestMap<i32> = serde_json::from_value(json).unwrap();

    assert_eq!(map, new_map)
}
