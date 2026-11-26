use super::last_page;

#[test]
fn page_navigation_counts_empty_exact_partial_and_capped_results() {
    for (total, size, expected) in [
        (0, 20, 1),
        (20, 20, 1),
        (21, 20, 2),
        (40, 20, 2),
        (1_000_001, 10, 100_000),
        (9_007_199_254_740_991, 100, 100_000),
    ] {
        assert_eq!(last_page(total, size), expected);
    }
}

#[test]
fn overlapping_pages_keep_the_first_row_for_each_id_and_page_one_replaces_the_list() {
    use super::merge_page;
    use serde_json::json;

    let a = json!({"id": 1, "title": "visible"});
    let duplicate = json!({"id": 1, "title": "later"});
    let b = json!({"id": 2});
    let c = json!({"id": 3});
    let mut rows = vec![a.clone()];
    merge_page(
        &mut rows,
        2,
        vec![duplicate.clone(), b.clone(), b.clone(), c.clone()],
    );
    assert_eq!(rows, [a, b, c]);
    merge_page(&mut rows, 1, vec![duplicate.clone()]);
    assert_eq!(rows, [duplicate]);
    merge_page(&mut rows, 1, vec![]);
    assert_eq!(rows, Vec::<serde_json::Value>::new());
}
