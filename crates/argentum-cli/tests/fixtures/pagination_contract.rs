// Public synthetic contract for the loopback CLI fixture, not a hidden grader.
use f10b_behavior_fixture::pagination::page_bounds;
use f10b_behavior_fixture::wrapper::{select_none, select_remaining};

#[test]
fn pagination_public_synthetic_boundaries() {
    for length in [0, 1, 7, usize::MAX] {
        assert_eq!(page_bounds(length, length, None), length..length);
        assert_eq!(page_bounds(length, usize::MAX, Some(4)), length..length);
        assert_eq!(page_bounds(length, 0, Some(0)), 0..0);
        assert_eq!(page_bounds(length, 0, None), 0..length);
    }
    assert_eq!(page_bounds(7, 2, Some(3)), 2..5);
    assert_eq!(page_bounds(7, 2, Some(usize::MAX)), 2..7);
    assert_eq!(page_bounds(usize::MAX, 2, Some(usize::MAX)), 2..usize::MAX);
    let values = vec!["x".to_owned(), "y".to_owned(), "z".to_owned()];
    assert_eq!(select_remaining(&values, 1), vec!["y", "z"]);
    assert!(select_none(&values, 1).is_empty());
}
